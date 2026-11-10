use modular_core::{dsp::schema, module_category::module_categories};

fn write_json<T: serde::Serialize>(path: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).expect("failed to serialize");
    std::fs::write(path, json).unwrap_or_else(|e| panic!("failed to write {path}: {e}"));
}

fn main() {
    let mut args = std::env::args().skip(1);
    let usage = "usage: generate-schemas <schemas-path> <categories-path>";
    let schemas_path = args.next().expect(usage);
    let categories_path = args.next().expect(usage);
    write_json(&schemas_path, &schema());
    write_json(&categories_path, &module_categories());
}
