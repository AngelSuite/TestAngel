use std::{collections::HashMap, path::Path};

use testangel_engine::ParameterValue;

/// Load the data from a CSV ready to processing. The data is
/// interpreted as a specific data type if possible, string otherwise.
///
/// # Errors
///
/// Return an error if the data cannot be loaded for any reason
pub fn load_data_spreadsheet<P>(path: P) -> csv::Result<Vec<HashMap<String, ParameterValue>>>
where
    P: AsRef<Path>,
{
    let mut reader = csv::Reader::from_path(path)?;
    let headers = reader
        .headers()?
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut data = vec![];

    for result in reader.into_records() {
        let record = result?;
        let mut d = HashMap::new();
        for (idx, field) in record.iter().enumerate() {
            let value = if field == "true" {
                ParameterValue::Boolean(true)
            } else if field == "false" {
                ParameterValue::Boolean(false)
            } else if let Ok(num) = field.parse::<f64>() {
                ParameterValue::Decimal(num)
            } else if let Ok(num) = field.parse::<i32>() {
                ParameterValue::Integer(num)
            } else {
                ParameterValue::String(field.to_string())
            };
            d.insert(headers[idx].clone(), value);
        }
        data.push(d);
    }

    Ok(data)
}
