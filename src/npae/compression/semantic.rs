use ndarray::Array1;
use crate::npae::compression::types::Stage1Out;

pub fn compress(s1: &Stage1Out) -> Result<Vec<f32>, String> {
    let dim = 256;
    let mut centroid = Array1::<f32>::zeros(dim);
    
    if s1.clean_tokens.is_empty() {
        return Ok(centroid.to_vec());
    }

    // simplistic ndarray usage to simulate embedding clustering
    for (i, token) in s1.clean_tokens.iter().enumerate() {
        let text_len = token.len() as f32;
        let val = (i as f32).sin() * text_len;
        for j in 0..dim {
            centroid[j] += val * (j as f32).cos();
        }
    }

    let n = s1.clean_tokens.len() as f32;
    centroid.mapv_inplace(|x| x / n);

    Ok(centroid.to_vec())
}
