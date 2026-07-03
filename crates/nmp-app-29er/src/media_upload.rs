//! App-owned media-upload helpers.
//!
//! `nmp-blossom` owns the reusable Blossom upload protocol. This module is the
//! small 29er adapter: parse the async `action_results` result body into the URL
//! that should be sent as group-message content.

/// Parse a Blossom upload terminal `result` JSON string into the URL 29er should
/// post into the selected group.
pub fn blossom_upload_url_from_result(result_json: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(result_json).map_err(|e| format!("parse Blossom result JSON: {e}"))?;
    let completion = nmp_blossom::parse_upload_completion(&value)?;
    let (url, _) = nmp_blossom::completion_url_sha256(&completion);
    if url.trim().is_empty() {
        return Err("Blossom upload result did not include a usable URL".to_string());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_server_upload_url() {
        let url = blossom_upload_url_from_result(
            r#"{"url":"https://b.example/photo.png","sha256":"abc","size":3,"type":"image/png","uploaded":1}"#,
        )
        .expect("single-server result parses");
        assert_eq!(url, "https://b.example/photo.png");
    }

    #[test]
    fn parses_first_successful_multi_server_upload_url() {
        let url = blossom_upload_url_from_result(
            r#"{"sha256":"abc","size":3,"type":"image/png","uploaded":1,"servers":[{"server":"https://a.example","ok":false,"error":"413"},{"server":"https://b.example","ok":true,"url":"https://b.example/photo.png"}]}"#,
        )
        .expect("multi-server result parses");
        assert_eq!(url, "https://b.example/photo.png");
    }
}
