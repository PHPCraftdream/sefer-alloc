use std::collections::HashMap;

/// Computes the median of a sorted slice.
/// Returns the middle element for odd length, or the average of the two middle elements for even length.
pub fn median(sorted_values: &[f64]) -> f64 {
    let len = sorted_values.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 1 {
        sorted_values[len / 2]
    } else {
        (sorted_values[len / 2 - 1] + sorted_values[len / 2]) / 2.0
    }
}

/// Computes the mean of a slice.
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

/// Sorts values and computes both mean and median.
pub fn mean_and_median(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (mean(&values), median(&values))
}

/// Prints raw CSV data.
/// `csv_prefix` is the first column (e.g., "raw_csv").
/// `labels` are the column headers (e.g., ["arm", "readers", "sample", "time_ns_per_op"]).
/// `rows` are tuples of values to print (must match labels count).
#[allow(dead_code)]
pub fn print_raw_csv(csv_prefix: &str, labels: &[&str], rows: &[Vec<&str>]) {
    print!("# {},", csv_prefix);
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            print!(",");
        }
        print!("{}", label);
    }
    println!();

    for row in rows {
        print!("{}", csv_prefix);
        for value in row {
            print!(",{}", value);
        }
        println!();
    }
}

/// Groups values by a key and returns a HashMap.
#[allow(dead_code)]
pub fn group_by_key<'a, K, V>(iter: impl IntoIterator<Item = (K, V)>) -> HashMap<K, Vec<V>>
where
    K: std::hash::Hash + Eq + Clone + 'a,
    V: Clone + 'a,
{
    let mut map: HashMap<K, Vec<V>> = HashMap::new();
    for (key, value) in iter {
        map.entry(key).or_default().push(value);
    }
    map
}
