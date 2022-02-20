use curl::easy::Easy;
use serde_json::value::Value;
use std::time::Duration;

use crate::errors;
use crate::errors::{Result, ResultExtInternal};

pub struct HttpResponse<T> {
    pub code: u32,
    pub content: T,
    pub headers: Vec<String>,
}

fn http_easy(mut easy: Easy) -> Result<HttpResponse<Vec<u8>>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut headers: Vec<String> = Vec::new();

    {
        let mut transfer = easy.transfer();

        transfer.header_function(|header| {
            headers.push(String::from_utf8_lossy(header).to_string());
            true
        })?;

        transfer.write_function(|data| {
            buf.extend_from_slice(data);
            Ok(data.len())
        })?;

        transfer.perform()?;
    }

    let code = easy.response_code()?;

    Ok(HttpResponse {
        code,
        content: buf,
        headers,
    })
}

pub fn http_get_socket_json(path: std::path::PathBuf, url: &str) -> Result<HttpResponse<Value>> {
    let mut easy = curl::easy::Easy::new();

    let cleaned_url = url.replace(' ', "%20");
    easy.url(&cleaned_url)?;
    easy.unix_socket_path(Some(path))?;

    let response = http_easy(easy)?;

    let content = serde_json::from_slice(&response.content)
        .internal_error("curl", "could not parse json response from server")?;

    Ok(HttpResponse {
        code: response.code,
        content,
        headers: response.headers,
    })
}

pub fn http_get_json(
    url: &str,
    timeout: Option<Duration>,
    request_headers: Vec<(&str, &str)>,
) -> Result<HttpResponse<Value>> {
    let mut easy = curl::easy::Easy::new();

    let cleaned_url = url.replace(' ', "%20");
    easy.url(&cleaned_url)?;

    if let Some(t) = timeout {
        easy.timeout(t)?;
    }

    let mut header_list = curl::easy::List::new();

    for (k, v) in request_headers.iter() {
        header_list.append(&format!("{}: {}", k, v))?;
    }

    easy.useragent("i3status")?;

    easy.http_headers(header_list)?;

    let response = http_easy(easy)?;

    let content = serde_json::from_slice(&response.content)
        .internal_error("curl", "could not parse json response from server")?;

    Ok(HttpResponse {
        code: response.code,
        content,
        headers: response.headers,
    })
}

impl From<curl::Error> for errors::Error {
    fn from(err: curl::Error) -> Self {
        errors::InternalError(
            "curl".to_owned(),
            "error running curl".to_owned(),
            Some((format!("{}", err), format!("{:?}", err))),
        )
    }
}
