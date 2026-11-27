//! The `.nam` file a `$nam` names, and the models built from it.

use std::borrow::Cow;
use std::path::Path;
use std::result::Result as StdResult;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use deserr::{DeserializeError, ErrorKind, IntoValue, ValuePointerRef};
use nam_sys::{NamDsp, NamFile};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::NamQuality;
use crate::patch::Patch;
use crate::types::{CableSource, Connect, LaneExpansion, PreviousIds};

/// A loaded `.nam` file. Each load gets a source no other load shares, so a
/// lane can tell whether the model it runs was built from these contents.
pub struct NamModelData {
    file: NamFile,
    source: u64,
}

impl NamModelData {
    /// Reads `path` and checks the runtime can build a model from it, so a
    /// broken file fails to load.
    pub fn load(path: &Path) -> Result<Self, String> {
        // 0 tags a lane that holds no model.
        static NEXT_SOURCE: AtomicU64 = AtomicU64::new(1);
        let file = NamFile::open(path)?;
        NamDsp::new(&file, 48_000.0, 1)?;
        Ok(Self {
            file,
            source: NEXT_SOURCE.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub fn file(&self) -> &NamFile {
        &self.file
    }

    pub fn source(&self) -> u64 {
        self.source
    }
}

/// The `model` param: the workspace path of a `.nam` file. The engine loads
/// it before the patch is built and hands it over in `prepare_resources`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NamModel {
    path: String,
}

impl NamModel {
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl<E: DeserializeError> deserr::Deserr<E> for NamModel {
    fn deserialize_from_value<V: IntoValue>(
        value: deserr::Value<V>,
        location: ValuePointerRef<'_>,
    ) -> StdResult<Self, E> {
        match value {
            deserr::Value::String(path) => Ok(Self { path }),
            _ => Err(deserr::take_cf_content(E::error::<V>(
                None,
                ErrorKind::Unexpected {
                    msg: "model must be the path of a .nam file".to_string(),
                },
                location,
            ))),
        }
    }
}

impl JsonSchema for NamModel {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("NamModel")
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        serde_json::from_value(serde_json::json!({ "type": "string" }))
            .expect("NamModel schema is valid JSON Schema")
    }
}

impl Connect for NamModel {
    fn apply_default_connections(&mut self) {}
    fn connect(&mut self, _patch: &Patch) {}
    fn collect_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn collect_lane_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn inject_index_ptr(&mut self, _ptr: *const std::cell::Cell<usize>) {}
    fn expand_lanes(&mut self, _expansion: &LaneExpansion) {}
    fn mark_glides(&mut self, _old: Option<&Self>, _generation: u64, _ids: &dyn PreviousIds) {}
}

/// The most samples one call into a model takes; longer blocks are split.
pub const MAX_FRAMES: usize = 64;

/// The loudness, in dB, `level` 0 brings every capture to, as NAM players do.
const TARGET_LOUDNESS_DB: f64 = -18.0;

/// A model with the factor that turns its output into volts at `level` 0.
pub struct LaneModel {
    pub dsp: NamDsp,
    pub out_scale: f32,
}

impl LaneModel {
    fn build(data: &NamModelData, quality: NamQuality, sample_rate: f32) -> Option<Self> {
        let mut dsp = NamDsp::new(data.file(), f64::from(sample_rate), MAX_FRAMES).ok()?;
        if quality == NamQuality::Lite {
            dsp.set_size(0.0);
        }
        let normalise = dsp.loudness().map_or(1.0, |loudness| {
            10f64.powf((TARGET_LOUDNESS_DB - loudness) / 20.0)
        });
        Some(Self {
            dsp,
            out_scale: (5.0 * normalise) as f32,
        })
    }
}

/// The most unplayed models kept for later modules; the oldest go first.
const POOL_MODELS: usize = 64;

/// Models built and prewarmed but never played, by [`Built::source`] and the
/// sample rate they run at. Each matches a newly built one exactly, so a
/// module takes from here before building.
static POOL: Mutex<Vec<(u64, u32, LaneModel)>> = Mutex::new(Vec::new());

fn pool() -> std::sync::MutexGuard<'static, Vec<(u64, u32, LaneModel)>> {
    POOL.lock().unwrap_or_else(|e| e.into_inner())
}

/// One ready model per lane, all from one file at one quality. A lane takes
/// its model in exchange for the one it held, which then drops with the
/// params on the main thread; models no lane took go back to the pool.
pub struct Built {
    /// The file's source and the quality together, so a lane keeps its model
    /// only while both hold.
    pub source: u64,
    sample_rate: u32,
    models: Vec<Option<LaneModel>>,
    /// Whether each slot still holds the model made for it, never played.
    unplayed: Vec<bool>,
}

impl Built {
    /// One model per lane for `data` at `quality`, from the pool where it
    /// has them, otherwise built and prewarmed. Main thread.
    pub fn new(data: &NamModelData, quality: NamQuality, sample_rate: f32, lanes: usize) -> Self {
        let source = data.source() * 2 + u64::from(quality == NamQuality::Lite);
        let rate = sample_rate.to_bits();
        let mut pooled: Vec<LaneModel> = {
            let mut pool = pool();
            let mut taken = Vec::with_capacity(lanes);
            let mut i = pool.len();
            while i > 0 && taken.len() < lanes {
                i -= 1;
                if pool[i].0 == source && pool[i].1 == rate {
                    taken.push(pool.remove(i).2);
                }
            }
            taken
        };
        let models: Vec<Option<LaneModel>> = (0..lanes)
            .map(|_| {
                pooled
                    .pop()
                    .or_else(|| LaneModel::build(data, quality, sample_rate))
            })
            .collect();
        Self {
            source,
            sample_rate: rate,
            unplayed: models.iter().map(Option::is_some).collect(),
            models,
        }
    }

    /// Swaps lane `lane`'s model for the one made for it, if that one was
    /// built. Never allocates.
    pub fn hand_over(&mut self, lane: usize, model: &mut Option<LaneModel>) -> bool {
        match self.models.get_mut(lane) {
            Some(slot @ Some(_)) => {
                std::mem::swap(slot, model);
                self.unplayed[lane] = false;
                true
            }
            _ => false,
        }
    }
}

impl Drop for Built {
    fn drop(&mut self) {
        let mut pool = pool();
        for (model, unplayed) in self.models.drain(..).zip(&self.unplayed) {
            if let (Some(model), true) = (model, *unplayed) {
                pool.push((self.source, self.sample_rate, model));
            }
        }
        let excess = pool.len().saturating_sub(POOL_MODELS);
        pool.drain(..excess);
    }
}

/// Params are cloned from the params cache before `prepare_resources` runs,
/// so a clone holds no models: they are made for each module afresh.
impl Clone for Built {
    fn clone(&self) -> Self {
        Self {
            source: self.source,
            sample_rate: self.sample_rate,
            models: Vec::new(),
            unplayed: Vec::new(),
        }
    }
}

impl Connect for Built {
    fn apply_default_connections(&mut self) {}
    fn connect(&mut self, _patch: &Patch) {}
    fn collect_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn collect_lane_cables(&self, _sink: &mut Vec<CableSource>) {}
    fn inject_index_ptr(&mut self, _ptr: *const std::cell::Cell<usize>) {}
    fn expand_lanes(&mut self, _expansion: &LaneExpansion) {}
    fn mark_glides(&mut self, _old: Option<&Self>, _generation: u64, _ids: &dyn PreviousIds) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// A newly loaded example capture, whose source no other test shares.
    fn data() -> NamModelData {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../nam_sys/NeuralAmpModelerCore/example_models/A2.nam");
        NamModelData::load(&path).expect("load example model")
    }

    fn pooled(source: u64) -> usize {
        pool().iter().filter(|(s, _, _)| *s == source).count()
    }

    fn render(model: &mut LaneModel) -> Vec<f32> {
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let mut output = vec![0.0; input.len()];
        assert!(model.dsp.process(&input, &mut output));
        output
    }

    #[test]
    fn unplayed_models_serve_the_next_module() {
        let data = data();
        let built = Built::new(&data, NamQuality::Full, RATE, 2);
        let source = built.source;
        assert_eq!(pooled(source), 0);
        drop(built);
        assert_eq!(pooled(source), 2);
        let again = Built::new(&data, NamQuality::Full, RATE, 2);
        assert_eq!(pooled(source), 0);
        assert!(again.models.iter().all(Option::is_some));
    }

    #[test]
    fn a_played_model_is_never_pooled() {
        let data = data();
        let mut built = Built::new(&data, NamQuality::Full, RATE, 2);
        let source = built.source;
        let mut lane = LaneModel::build(&data, NamQuality::Full, RATE);
        assert!(built.hand_over(0, &mut lane));
        drop(built);
        assert_eq!(pooled(source), 1);
    }

    #[test]
    fn a_pooled_model_sounds_like_a_new_one() {
        let data = data();
        drop(Built::new(&data, NamQuality::Full, RATE, 1));
        let mut from_pool = Built::new(&data, NamQuality::Full, RATE, 1);
        let mut pooled_model = None;
        assert!(from_pool.hand_over(0, &mut pooled_model));
        let mut fresh = LaneModel::build(&data, NamQuality::Full, RATE).expect("build");
        assert_eq!(
            render(pooled_model.as_mut().expect("pooled")),
            render(&mut fresh)
        );
    }
}
