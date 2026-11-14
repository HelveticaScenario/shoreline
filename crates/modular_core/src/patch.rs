//! Core patch structure for DSP processing
//!
//! This module contains the core `Patch` struct which represents a graph of
//! connected audio modules. The patch contains sampleable modules and tracks
//! that can be processed to generate audio.

use crate::bounds::Settled;
use crate::dsp::core::audio_in::AudioIn;
use crate::types::{
    CableSource, Message, MessageTag, ROOT_ID, ROOT_OUTPUT_PORT, Sampleable, SampleableMap, WavData,
};

use std::collections::HashMap;
use std::sync::Arc;

/// The core patch structure containing the DSP graph
pub struct Patch {
    pub sampleables: SampleableMap,
    pub wav_data: HashMap<String, Arc<WavData>>,
    message_listeners: HashMap<MessageTag, Vec<String>>,
    /// Which outputs are bounded and read, from the last
    /// [`insert_modules`](Self::insert_modules). Main thread only: a module
    /// rebuilt on its own settles from here.
    pub bounds: PatchBounds,
}

/// Which outputs of a patch are bounded and ranged so far, as bits by port
/// index per module, while the patch is settled.
#[derive(Default)]
struct SettledMasks<'a> {
    bounded: HashMap<&'a str, u64>,
    ranged: HashMap<&'a str, u64>,
}

/// [`SettledMasks`] read by port name, through `index`.
struct MaskView<'a> {
    masks: &'a SettledMasks<'a>,
    index: &'a dyn Fn(&str, &str) -> Option<usize>,
}

impl MaskView<'_> {
    fn has(&self, masks: &HashMap<&str, u64>, module: &str, port: &str) -> bool {
        (self.index)(module, port)
            .is_some_and(|i| masks.get(module).is_some_and(|mask| mask & (1 << i) != 0))
    }
}

impl Settled for MaskView<'_> {
    fn bounded(&self, module: &str, port: &str) -> bool {
        self.has(&self.masks.bounded, module, port)
    }

    fn ranged(&self, module: &str, port: &str) -> bool {
        self.has(&self.masks.ranged, module, port)
    }
}

/// Which outputs of a patch are bounded and ranged, and which have their
/// bounds read.
#[derive(Clone, Debug, Default)]
pub struct PatchBounds {
    /// The bounded outputs, as `(module, port)`, each with whether it is
    /// ranged: its bounds swing.
    pub outputs: HashMap<(String, String), bool>,
    /// The outputs whose bounds are read, as bits by port index per module;
    /// those whose module computes them store them.
    pub read: HashMap<String, u64>,
}

impl Settled for PatchBounds {
    fn bounded(&self, module: &str, port: &str) -> bool {
        self.outputs
            .contains_key(&(module.to_string(), port.to_string()))
    }

    fn ranged(&self, module: &str, port: &str) -> bool {
        self.outputs
            .get(&(module.to_string(), port.to_string()))
            .is_some_and(|&ranged| ranged)
    }
}

impl PatchBounds {
    /// Settle a module rebuilt on its own, under the same id, as its
    /// predecessor was settled. Main thread, before it is shipped.
    pub fn settle_rebuilt(&self, id: &str, module: &dyn Sampleable) {
        module.settle_params(self);
        module.make_bounded(self.read.get(id).copied().unwrap_or(0));
    }
}

impl Default for Patch {
    fn default() -> Self {
        Self::new()
    }
}

impl Patch {
    /// Create a new empty patch
    pub fn new() -> Self {
        let mut sampleables: SampleableMap = Default::default();
        let audio_in_sampleable = AudioIn::default();

        sampleables.insert(
            audio_in_sampleable.get_id().to_string(),
            Box::new(audio_in_sampleable),
        );
        let mut patch = Patch {
            sampleables,
            wav_data: HashMap::new(),
            message_listeners: HashMap::new(),
            bounds: PatchBounds::default(),
        };
        patch.rebuild_message_listeners();
        patch
    }

    pub fn rebuild_message_listeners(&mut self) {
        self.message_listeners.clear();
        let ids: Vec<String> = self.sampleables.keys().cloned().collect();
        for id in ids {
            self.add_message_listeners_for_module(&id);
        }
    }

    /// Add message listener entries for a single module (incremental update).
    pub fn add_message_listeners_for_module(&mut self, id: &str) {
        let Some(sampleable) = self.sampleables.get(id) else {
            return;
        };

        for tag in sampleable.handled_message_tags() {
            self.message_listeners
                .entry(*tag)
                .or_default()
                .push(id.to_string());
        }
    }

    /// Remove all message listener entries for a given module id.
    pub fn remove_message_listeners_for_module(&mut self, module_id: &str) {
        for listeners in self.message_listeners.values_mut() {
            listeners.retain(|id| id != module_id);
        }
    }

    pub fn dispatch_message(&mut self, message: &Message) -> napi::Result<()> {
        let Some(listener_ids) = self.message_listeners.get(&message.tag()) else {
            return Ok(());
        };

        for id in listener_ids {
            if let Some(sampleable) = self.sampleables.get(id) {
                sampleable.handle_message(message)?;
            }
        }

        Ok(())
    }

    /// Get the output sample from the root module (lane 0, slot 0).
    pub fn get_output(&self) -> f32 {
        self.sampleables
            .get(&*ROOT_ID)
            .map(|root| root.get_value_at(&ROOT_OUTPUT_PORT, 0, 0))
            .unwrap_or_default()
    }

    /// Construct each module from already-deserialized params at its resolved
    /// processing mode and insert it unconnected. Main thread only.
    pub fn insert_modules(
        &mut self,
        modules: impl IntoIterator<
            Item = (
                String,
                String,
                crate::params::DeserializedParams,
                crate::types::ProcessingMode,
            ),
        >,
        sample_rate: f32,
        block_size: usize,
    ) -> Result<(), String> {
        let constructors = crate::dsp::get_constructors();
        let mut modules: Vec<_> = modules.into_iter().collect();
        settle_channels(&mut modules)?;
        for (id, module_type, params, mode) in modules {
            let constructor = constructors
                .get(&module_type)
                .ok_or_else(|| format!("Unknown module type: {}", module_type))?;
            let module = constructor(&id, sample_rate, params, block_size, mode)
                .map_err(|e| format!("Failed to create module {}: {}", id, e))?;
            self.sampleables.insert(id, module);
        }
        self.settle_bounds();
        Ok(())
    }

    /// Settle which outputs carry bounds and give them their storage.
    ///
    /// An output is bounded when its range or params bound it, or when every
    /// input its bounds come from is bounded; it is ranged as well when one
    /// of those inputs is ranged rather than constant. Growing both sets
    /// together until nothing joins takes the least such sets, so a feedback
    /// cycle that only feeds itself never becomes bounded. A bounded output
    /// stores its bounds only when something reads them: a module that reads
    /// an input's bounds for itself, or a read output whose bounds come from
    /// it. Each module then settles the choices it makes from which of its
    /// inputs are bounded or ranged. The result is left in
    /// [`bounds`](Self::bounds).
    ///
    /// Main thread, once the modules are constructed and before
    /// [`connect_all`](Self::connect_all): it replaces the storage cables
    /// point into.
    fn settle_bounds(&mut self) {
        let index = |module: &str, port: &str| {
            self.sampleables
                .get(module)
                .and_then(|producer| producer.port_index(port))
        };
        let mut masks = SettledMasks::default();
        loop {
            let mut grew = false;
            for (id, module) in &self.sampleables {
                let settled = MaskView {
                    masks: &masks,
                    index: &index,
                };
                let bounded = module.bounded_outputs(&settled);
                let ranged = module.ranged_outputs(&settled) & bounded;
                for (set, mask) in [(&mut masks.bounded, bounded), (&mut masks.ranged, ranged)] {
                    let entry = set.entry(id.as_str()).or_default();
                    if mask & !*entry != 0 {
                        *entry |= mask;
                        grew = true;
                    }
                }
            }
            if !grew {
                break;
            }
        }
        let outputs = masks
            .bounded
            .iter()
            .flat_map(|(id, &mask)| {
                let ranged = masks.ranged.get(id).copied().unwrap_or(0);
                let names = self.sampleables[*id].port_names();
                names
                    .iter()
                    .enumerate()
                    .filter(move |(i, _)| mask & (1 << i) != 0)
                    .map(move |(i, port)| {
                        ((id.to_string(), port.to_string()), ranged & (1 << i) != 0)
                    })
            })
            .collect();
        let mut settled = PatchBounds {
            outputs,
            read: HashMap::new(),
        };
        for module in self.sampleables.values() {
            module.settle_params(&settled);
        }
        let mut stored: HashMap<String, u64> = HashMap::new();
        let mut queue: Vec<CableSource> = self
            .sampleables
            .values()
            .flat_map(|module| module.bounds_sources(0))
            .collect();
        while let Some(source) = queue.pop() {
            let Some(i) = index(&source.module, &source.port) else {
                continue;
            };
            let bit = 1u64 << i;
            if masks
                .bounded
                .get(source.module.as_str())
                .is_none_or(|mask| mask & bit == 0)
            {
                continue;
            }
            let entry = stored.entry(source.module.clone()).or_default();
            if *entry & bit != 0 {
                continue;
            }
            *entry |= bit;
            queue.extend(self.sampleables[&source.module].bounds_sources(bit));
        }
        for (id, module) in &self.sampleables {
            module.make_bounded(stored.get(id).copied().unwrap_or(0));
        }
        settled.read = stored;
        self.bounds = settled;
    }

    /// Resolve every module's cable pointers, then notify each that the patch is
    /// ready. `connect` reads raw pointers into upstream modules' output buffers,
    /// so on a hot-swap this MUST run only after `transfer_state_from` has
    /// swapped those buffers in. Allocation-free and safe on the audio thread.
    pub fn connect_all(&self) {
        for module in self.sampleables.values() {
            module.connect(self);
        }
        for module in self.sampleables.values() {
            module.on_patch_update();
        }
    }
}

/// Settle every module's channels per lane and map its inputs onto their
/// producers' physical lanes, before construction.
///
/// A `stereo` module is stereo and a `mono` one mono. A lifted module is
/// stereo when any input it reads lane by lane comes from a stereo output, so
/// the analysis grows from the stereo modules until nothing changes; a
/// feedback cycle with no stereo source stays mono. An output follows its
/// module's channels unless it has a fixed width or a single value. Each
/// module's lane count then counts sides, and its inputs are rewritten by
/// [`Connect::expand_lanes`](crate::types::Connect::expand_lanes).
fn settle_channels(
    modules: &mut [(
        String,
        String,
        crate::params::DeserializedParams,
        crate::types::ProcessingMode,
    )],
) -> Result<(), String> {
    use crate::types::StereoMode;
    let layouts = crate::dsp::stereo_layouts();
    let types: HashMap<String, String> = modules
        .iter()
        .map(|(id, module_type, ..)| (id.clone(), module_type.clone()))
        .collect();
    let mode = |id: &str| {
        types
            .get(id)
            .and_then(|module_type| layouts.get(module_type))
            .map_or(StereoMode::Mono, |layout| layout.mode)
    };
    let mut channels: HashMap<String, usize> = modules
        .iter()
        .map(|(id, _, params, _)| {
            let stereo = match mode(id) {
                StereoMode::Stereo => true,
                StereoMode::Derived => {
                    let mut files = Vec::new();
                    params.params.collect_stereo_files(&mut files);
                    !files.is_empty()
                }
                StereoMode::Lift | StereoMode::Mono => false,
            };
            (id.clone(), if stereo { 2 } else { 1 })
        })
        .collect();
    let lane_cables: Vec<(String, Vec<crate::types::CableSource>)> = modules
        .iter()
        .filter(|(id, ..)| mode(id) == StereoMode::Lift)
        .map(|(id, _, params, _)| {
            let mut sink = Vec::new();
            params.params.collect_lane_cables(&mut sink);
            (id.clone(), sink)
        })
        .collect();
    let producer = |channels: &HashMap<String, usize>, module: &str, port: &str| {
        let follows = types
            .get(module)
            .and_then(|module_type| layouts.get(module_type))
            .is_some_and(|layout| layout.following_ports.contains(port));
        if follows {
            channels.get(module).copied().unwrap_or(1)
        } else {
            1
        }
    };
    loop {
        let mut grew = false;
        for (id, cables) in &lane_cables {
            let stereo = cables
                .iter()
                .any(|cable| producer(&channels, &cable.module, &cable.port) == 2);
            if stereo && channels[id] == 1 {
                channels.insert(id.clone(), 2);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let producer_channels = |module: &str, port: &str| producer(&channels, module, port);
    for (id, _, params, _) in modules.iter_mut() {
        let module_channels = channels[id.as_str()];
        let lanes = params.lane_count.max(1);
        if lanes * module_channels > crate::poly::PORT_MAX_LANES {
            return Err(format!(
                "{id} is stereo with {lanes} lanes, more than the {} a stereo signal can carry",
                crate::poly::PORT_MAX_LANES / module_channels
            ));
        }
        params.params.expand_lanes(&crate::types::LaneExpansion {
            channels: module_channels,
            producer: &producer_channels,
        });
        params.lane_count *= module_channels;
        params.channels = module_channels;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::types::{MessageHandler, WellKnownModule};
    use napi::Result;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex as StdMutex, OnceLock};

    struct CountingAllocator;

    static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
    static TRACKING_ENABLED: AtomicUsize = AtomicUsize::new(0);
    static TRACKING_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();

    #[global_allocator]
    static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if TRACKING_ENABLED.load(Ordering::Relaxed) != 0 {
                ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    fn allocation_tracking_lock() -> &'static StdMutex<()> {
        TRACKING_LOCK.get_or_init(|| StdMutex::new(()))
    }

    fn assert_message_listener_dispatch_does_not_allocate() {
        let hits = Arc::new(AtomicUsize::new(0));
        let s: Box<dyn Sampleable> = Box::new(CountingMessageSampleable {
            id: "m1".to_string(),
            hits: Arc::clone(&hits),
        });

        let mut patch = Patch::new();
        patch.sampleables.insert("m1".to_string(), s);
        patch.rebuild_message_listeners();

        let message = Message::MidiNoteOn(crate::types::MidiNoteOn {
            device: None,
            note: 60,
            velocity: 100,
            channel: 0,
        });

        let _guard = allocation_tracking_lock().lock().unwrap();
        ALLOCATIONS.store(0, Ordering::SeqCst);
        TRACKING_ENABLED.store(1, Ordering::SeqCst);
        patch.dispatch_message(&message).unwrap();
        TRACKING_ENABLED.store(0, Ordering::SeqCst);

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_patch_new_has_hidden_audio_in() {
        let patch = Patch::new();
        // Patch::new() inserts HIDDEN_AUDIO_IN which is managed internally
        assert!(
            patch
                .sampleables
                .contains_key(WellKnownModule::HiddenAudioIn.id())
        );
        assert_eq!(patch.sampleables.len(), 1);
    }

    #[test]
    fn test_patch_get_output_no_root() {
        let patch = Patch::new();
        let output = patch.get_output();
        assert!(
            (output - 0.0).abs() < 0.0001,
            "No root module should return 0.0"
        );
    }

    struct DummyMessageSampleable {
        id: String,
    }

    impl Sampleable for DummyMessageSampleable {
        fn get_id(&self) -> &str {
            &self.id
        }

        fn get_module_type(&self) -> &str {
            "dummy"
        }

        fn connect(&self, _patch: &Patch) {}

        fn start_block(&self) {}

        fn ensure_processed_to(&self, _target: usize) {}

        fn ensure_processed(&self) {}

        fn get_value_at(&self, _port: &str, _ch: usize, _index: usize) -> f32 {
            0.0
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl MessageHandler for DummyMessageSampleable {
        fn handled_message_tags(&self) -> &'static [MessageTag] {
            &[MessageTag::MidiNoteOn]
        }

        fn handle_message(&self, _message: &Message) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn message_listener_index_stores_ids_only() {
        let s: Box<dyn Sampleable> = Box::new(DummyMessageSampleable {
            id: "m1".to_string(),
        });

        let mut patch = Patch::new();
        patch.sampleables.insert("m1".to_string(), s);
        patch.rebuild_message_listeners();

        assert_eq!(
            patch
                .message_listeners
                .get(&MessageTag::MidiNoteOn)
                .cloned(),
            Some(vec!["m1".to_string()])
        );
    }

    struct CountingMessageSampleable {
        id: String,
        hits: Arc<AtomicUsize>,
    }

    impl Sampleable for CountingMessageSampleable {
        fn get_id(&self) -> &str {
            &self.id
        }

        fn get_module_type(&self) -> &str {
            "counting"
        }

        fn connect(&self, _patch: &Patch) {}

        fn start_block(&self) {}

        fn ensure_processed_to(&self, _target: usize) {}

        fn ensure_processed(&self) {}

        fn get_value_at(&self, _port: &str, _ch: usize, _index: usize) -> f32 {
            0.0
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl MessageHandler for CountingMessageSampleable {
        fn handled_message_tags(&self) -> &'static [MessageTag] {
            &[MessageTag::MidiNoteOn]
        }

        fn handle_message(&self, _message: &Message) -> Result<()> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn message_listener_removed_module_is_not_dispatched() {
        let hits = Arc::new(AtomicUsize::new(0));
        let s: Box<dyn Sampleable> = Box::new(CountingMessageSampleable {
            id: "m1".to_string(),
            hits: Arc::clone(&hits),
        });

        let mut patch = Patch::new();
        patch.sampleables.insert("m1".to_string(), s);
        patch.rebuild_message_listeners();

        let message = Message::MidiNoteOn(crate::types::MidiNoteOn {
            device: None,
            note: 60,
            velocity: 100,
            channel: 0,
        });

        patch.dispatch_message(&message).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        patch.sampleables.remove("m1");
        patch.remove_message_listeners_for_module("m1");

        patch.dispatch_message(&message).unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert_eq!(
            patch
                .message_listeners
                .get(&MessageTag::MidiNoteOn)
                .map(Vec::len),
            Some(0)
        );
    }

    #[test]
    fn message_listener_dispatch_does_not_allocate() {
        const ISOLATED_ALLOC_TEST_ENV: &str = "MODULAR_CORE_ISOLATED_ALLOC_TEST";

        if std::env::var_os(ISOLATED_ALLOC_TEST_ENV).is_some() {
            assert_message_listener_dispatch_does_not_allocate();
            return;
        }

        // The allocator counter is process-global, so run the actual assertion in
        // a child test process to avoid unrelated allocations from concurrently
        // executing tests in the parent harness.
        let output = Command::new(std::env::current_exe().unwrap())
            .env(ISOLATED_ALLOC_TEST_ENV, "1")
            .arg("--exact")
            .arg("patch::tests::message_listener_dispatch_does_not_allocate")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .output()
            .expect("failed to spawn isolated allocation test");

        assert!(
            output.status.success(),
            "isolated allocation test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
