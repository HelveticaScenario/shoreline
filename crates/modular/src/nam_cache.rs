//! The `.nam` files `$nam` modules name, loaded on the main thread before a
//! patch is built and kept while each file's mtime holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use modular_core::dsp::fx::nam::NamModelData;
use modular_core::types::PatchGraph;

use crate::validation::ValidationError;

struct Entry {
    data: Arc<NamModelData>,
    mtime: SystemTime,
}

#[derive(Default)]
pub struct NamCache {
    entries: HashMap<String, Entry>,
}

impl NamCache {
    /// Loads the model every `$nam` in `graph` names, by the path its
    /// `model` param holds, through `resolve`. Errors name the module.
    pub fn load_for(
        &mut self,
        graph: &PatchGraph,
        resolve: impl Fn(&str) -> napi::Result<PathBuf>,
    ) -> Result<HashMap<String, Arc<NamModelData>>, Vec<ValidationError>> {
        let mut loaded = HashMap::new();
        let mut errors = Vec::new();
        for module in graph.modules.iter().filter(|m| m.module_type == "$nam") {
            let Some(path) = module.params.get("model").and_then(|v| v.as_str()) else {
                continue;
            };
            if loaded.contains_key(path) {
                continue;
            }
            match resolve(path)
                .map_err(|e| e.reason.clone())
                .and_then(|file| self.load(path, &file))
            {
                Ok(data) => {
                    loaded.insert(path.to_string(), data);
                }
                Err(message) => errors.push(ValidationError {
                    field: "model".to_string(),
                    message,
                    location: None,
                    expected_type: None,
                    actual_value: Some(format!("{path:?}")),
                    module_id: Some(module.id.clone()),
                }),
            }
        }
        if errors.is_empty() {
            Ok(loaded)
        } else {
            Err(errors)
        }
    }

    /// Every cached model, for rebuilding one module outside a patch build.
    pub fn snapshot(&self) -> HashMap<String, Arc<NamModelData>> {
        self.entries
            .iter()
            .map(|(path, entry)| (path.clone(), Arc::clone(&entry.data)))
            .collect()
    }

    /// Forgets every workspace file, as when the workspace folder changes.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn load(&mut self, path: &str, file: &Path) -> Result<Arc<NamModelData>, String> {
        let mtime = std::fs::metadata(file)
            .and_then(|m| m.modified())
            .map_err(|e| format!("cannot read {path}: {e}"))?;
        if let Some(entry) = self.entries.get(path).filter(|e| e.mtime == mtime) {
            return Ok(Arc::clone(&entry.data));
        }
        let data =
            Arc::new(NamModelData::load(file).map_err(|e| format!("cannot load {path}: {e}"))?);
        self.entries.insert(
            path.to_string(),
            Entry {
                data: Arc::clone(&data),
                mtime,
            },
        );
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use modular_core::types::ModuleSpec;

    fn graph(paths: &[&str]) -> PatchGraph {
        PatchGraph {
            modules: paths
                .iter()
                .enumerate()
                .map(|(i, path)| ModuleSpec {
                    id: format!("amp{i}"),
                    module_type: "$nam".to_string(),
                    id_is_explicit: None,
                    skip_state_transfer: None,
                    params: serde_json::json!({ "input": 0.0, "model": path }),
                })
                .collect(),
            module_id_remaps: None,
            scopes: Vec::new(),
            vu_meters: Vec::new(),
        }
    }

    fn models() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../nam_sys/NeuralAmpModelerCore/example_models")
    }

    fn resolve(path: &str) -> napi::Result<PathBuf> {
        Ok(models().join(path.trim_start_matches('/')))
    }

    #[test]
    fn loads_each_named_model_once_and_keeps_it() {
        let mut cache = NamCache::default();
        let first = cache
            .load_for(&graph(&["/A2.nam", "/A2.nam", "/lstm.nam"]), resolve)
            .expect("load");
        assert_eq!(first.len(), 2);
        let again = cache.load_for(&graph(&["/A2.nam"]), resolve).expect("load");
        assert!(Arc::ptr_eq(&first["/A2.nam"], &again["/A2.nam"]));
        assert_eq!(cache.snapshot().len(), 2);
    }

    #[test]
    fn a_missing_or_broken_model_names_its_module() {
        let dir = std::env::temp_dir().join(format!("nam_cache_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        std::fs::write(dir.join("broken.nam"), r#"{"version": "0.5.0"}"#).expect("write");
        let mut cache = NamCache::default();
        let errors = cache
            .load_for(
                &graph(&["/A2.nam", "/missing.nam", "/broken.nam"]),
                |path| {
                    let name = path.trim_start_matches('/');
                    Ok(if name == "A2.nam" {
                        models()
                    } else {
                        dir.clone()
                    }
                    .join(name))
                },
            )
            .err()
            .expect("two models fail");
        let _ = std::fs::remove_dir_all(&dir);
        let modules: Vec<_> = errors
            .iter()
            .filter_map(|e| e.module_id.as_deref())
            .collect();
        assert_eq!(modules, ["amp1", "amp2"]);
        assert!(errors.iter().all(|e| e.field == "model"));
    }
}
