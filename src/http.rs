use std::time::Duration;
use serde_json::value::Value;

use crate::errors::{Result, ResultExtInternal};
use crate::errors;



pub fn http_get(url: &str, timeout: Duration) -> Result<(u32, String)> {
    let mut buf: Vec<u8> = Vec::new();
    let mut easy = curl::easy::Easy::new();

    easy.url(url)?;
    easy.timeout(timeout)?;

    {
        let mut transfer = easy.transfer();

        (transfer.write_function(|data| {
            buf.extend_from_slice(data);
            Ok(data.len())
        }))?;

        transfer.perform()?;
    }

    let response_code = easy.response_code()?;

    let response_str = String::from_utf8(buf)
        .internal_error("curl", "Received non-UTF8 characters in http response")?;

    Ok((response_code, response_str))
}

pub struct HttpResponse<T> {
    pub code: u32,
    pub content: T,
    pub headers: Vec<String>,
}

pub fn http_get_json(url: &str, timeout: Duration, request_headers: Vec<(&str, &str)>) -> Result<HttpResponse<Value>> {
    let mut buf: Vec<u8> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut easy = curl::easy::Easy::new();

    easy.url(url)?;
    easy.timeout(timeout)?;

    let mut header_list = curl::easy::List::new();

    for (k, v) in request_headers.iter() {
        header_list.append(&format!("{}: {}", k, v))?;
    }

    easy.http_headers(header_list)?;

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

    let content = serde_json::from_slice(&buf)
        .internal_error("curl", "could not parse json response from server")?;

    Ok(HttpResponse { code, content, headers })
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
