use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;

pub fn load_json_file<T: DeserializeOwned>(
    path: impl AsRef<Path>,
) -> Result<T, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let value = serde_json::from_str::<T>(&text)?;
    Ok(value)
}
