use base64::engine::general_purpose;
use base64::Engine;
use serde_yaml::{from_str, to_string, Value};
use std::fs::File;
use std::io::{Read, Write};

// 加载本地配置
pub(crate) fn load_local_config(path: &str) -> anyhow::Result<Value> {
    let path = std::path::Path::new(path);
    if !path.exists() {
        // 创建默认空配置
        let default_config = Value::Mapping(Default::default());
        save_local_config(path.to_str().unwrap(), &default_config)?;
    }

    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    from_str(&contents).map_err(|e| anyhow::anyhow!("Failed to parse YAML: {}", e))
}

// 保存本地配置
pub(crate) fn save_local_config(path: &str, config: &Value) -> anyhow::Result<()> {
    let yaml = to_string(config)?;
    let mut file = File::create(path)?;
    file.write_all(yaml.as_bytes())?;
    Ok(())
}

// 获取远程配置
pub(crate) async fn fetch_remote_config(url: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let response = reqwest::get(url).await?;
    let base64_content = response.text().await?;

    let decoded_bytes = general_purpose::STANDARD.decode(base64_content.trim())?;
    let decoded_str = String::from_utf8(decoded_bytes)?;

    from_str(&decoded_str).map_err(|e| e.into())
}

// 合并配置
pub(crate) fn merge_configs(
    mut remote_config: Value,
    local_config: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    process_prepend_append(&mut remote_config, local_config);
    Ok(remote_config)
}

// 处理前置/后置操作
fn process_prepend_append(remote_config: &mut Value, local_config: &Value) {
    if let Value::Mapping(local_map) = local_config {
        for (key, value) in local_map {
            if let Some(key_str) = key.as_str() {
                if key_str.starts_with("prepend-") {
                    let target_key = &key_str[8..];
                    prepend_to_array(remote_config, target_key, value);
                } else if key_str.starts_with("append-") {
                    let target_key = &key_str[7..];
                    append_to_array(remote_config, target_key, value);
                }
            }
        }
    }
}

// 前置操作
fn prepend_to_array(config: &mut Value, target_key: &str, prepend_value: &Value) {
    if let Value::Mapping(config_map) = config {
        // 使用 entry API 获取可变引用
        let target_array = config_map.get_mut(&Value::String(target_key.to_string()));

        match target_array {
            Some(Value::Sequence(target_seq)) => {
                if let Value::Sequence(prepend_seq) = prepend_value {
                    let mut new_seq = prepend_seq.clone();
                    new_seq.extend(target_seq.clone());
                    *target_seq = new_seq;
                }
            }
            None => {
                if let Value::Sequence(prepend_seq) = prepend_value {
                    // 创建新序列
                    config_map.insert(
                        Value::String(target_key.to_string()),
                        Value::Sequence(prepend_seq.clone()),
                    );
                }
            }
            _ => {} // 其他情况忽略
        }
    }
}

// 后置操作
fn append_to_array(config: &mut Value, target_key: &str, append_value: &Value) {
    if let Value::Mapping(config_map) = config {
        // 使用 entry API 获取可变引用
        let target_array = config_map.get_mut(&Value::String(target_key.to_string()));

        match target_array {
            Some(Value::Sequence(target_seq)) => {
                if let Value::Sequence(append_seq) = append_value {
                    // 直接扩展现有序列
                    target_seq.extend(append_seq.clone());
                }
            }
            None => {
                if let Value::Sequence(append_seq) = append_value {
                    // 创建新序列
                    config_map.insert(
                        Value::String(target_key.to_string()),
                        Value::Sequence(append_seq.clone()),
                    );
                }
            }
            _ => {} // 其他情况忽略
        }
    }
}
