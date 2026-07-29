use reqwest::{
    ClientBuilder, Response, Url,
    header::{ACCEPT, HeaderValue},
};

use super::{
    CalendarError,
    auth::{Auth, Authorize, AuthorizeUrl},
    ical::{Event, parse_events},
};
use crate::{APP_USER_AGENT, REQWEST_TIMEOUT};

const MAX_ICS_SIZE: usize = 32 * 1024 * 1024;

pub struct Client {
    url: Url,
    client: reqwest::Client,
    auth: Auth,
}

impl Client {
    pub fn new(url: Url, auth: Auth) -> Self {
        Self {
            url,
            client: ClientBuilder::new()
                .user_agent(APP_USER_AGENT)
                .timeout(REQWEST_TIMEOUT)
                .build()
                .expect("A valid HTTP client"),
            auth,
        }
    }

    pub async fn events(
        &mut self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Event>, CalendarError> {
        let mut retries = 0;
        loop {
            let request = self
                .client
                .get(self.url.clone())
                .headers(self.auth.headers().await)
                .header(ACCEPT, HeaderValue::from_static("text/calendar"))
                .build()
                .expect("A valid ICS request");
            // The URL of an ICS feed can contain a secret token. Reqwest includes request URLs in
            // some errors, so strip it before the error can reach the block output or logs.
            let result = self
                .client
                .execute(request)
                .await
                .map_err(redact_http_error)?;
            match result.error_for_status() {
                Err(error) if retries == 0 => {
                    self.auth.handle_error(error.without_url()).await?;
                    retries += 1;
                }
                Err(error) => return Err(redact_http_error(error)),
                Ok(result) => {
                    let body = read_body(result, MAX_ICS_SIZE).await?;
                    let body = std::str::from_utf8(&body).map_err(|error| {
                        CalendarError::Parsing(format!(
                            "iCalendar feed is not valid UTF-8: {error}"
                        ))
                    })?;
                    return parse_events(body, start, end);
                }
            }
        }
    }

    pub async fn authorize(&mut self) -> Result<Authorize, CalendarError> {
        self.auth.authorize().await
    }

    pub async fn ask_user(&mut self, authorize: AuthorizeUrl) -> Result<(), CalendarError> {
        self.auth.ask_user(authorize).await
    }
}

fn redact_http_error(error: reqwest::Error) -> CalendarError {
    CalendarError::Http(error.without_url())
}

async fn read_body(mut response: Response, limit: usize) -> Result<Vec<u8>, CalendarError> {
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > limit as u64) {
        return Err(size_limit_error(limit));
    }

    let initial_capacity = content_length
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default();
    let mut body = Vec::with_capacity(initial_capacity);
    while let Some(chunk) = response.chunk().await.map_err(redact_http_error)? {
        if body
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > limit)
        {
            return Err(size_limit_error(limit));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn size_limit_error(limit: usize) -> CalendarError {
    CalendarError::Parsing(format!(
        "iCalendar feed exceeds the {limit}-byte size limit"
    ))
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::TcpListener,
    };

    use super::*;

    async fn serve_response(
        path: &str,
        response: String,
    ) -> (Url, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 4096];
            let read = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..read]).into_owned();
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });
        (
            Url::parse(&format!("http://{address}{path}")).unwrap(),
            handle,
        )
    }

    #[tokio::test]
    async fn fetches_and_parses_ics_over_http() {
        let body = concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:event\r\n",
            "DTSTART:20260729T160000Z\r\n",
            "DTEND:20260729T170000Z\r\n",
            "SUMMARY:Test\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );
        let (url, request) = serve_response(
            "/calendar.ics",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/calendar\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        let mut client = Client::new(url, Auth::Unauthenticated);
        let start = chrono::DateTime::parse_from_rfc3339("2026-07-29T15:00:00Z")
            .unwrap()
            .to_utc();
        let end = chrono::DateTime::parse_from_rfc3339("2026-07-30T15:00:00Z")
            .unwrap()
            .to_utc();

        let events = client.events(start, end).await.unwrap();
        let request = request.await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary.as_deref(), Some("Test"));
        assert!(request.starts_with("GET /calendar.ics HTTP/1.1\r\n"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("\r\naccept: text/calendar\r\n")
        );
    }

    #[tokio::test]
    async fn http_errors_do_not_expose_private_feed_url() {
        const SECRET: &str = "private-secret-token";
        let (url, request) = serve_response(
            &format!("/calendar/ical/{SECRET}/basic.ics"),
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
        )
        .await;
        let mut client = Client::new(url, Auth::Unauthenticated);
        let start = chrono::DateTime::parse_from_rfc3339("2026-07-29T15:00:00Z")
            .unwrap()
            .to_utc();
        let end = chrono::DateTime::parse_from_rfc3339("2026-07-30T15:00:00Z")
            .unwrap()
            .to_utc();

        let error = client.events(start, end).await.unwrap_err();
        request.await.unwrap();

        assert!(matches!(error, CalendarError::Http(_)));
        assert!(!error.to_string().contains(SECRET));
    }

    #[tokio::test]
    async fn body_errors_do_not_expose_private_feed_url() {
        const SECRET: &str = "private-secret-token";
        let (url, request) = serve_response(
            &format!("/calendar/ical/{SECRET}/basic.ics"),
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\ninvalid\r\n"
                .into(),
        )
        .await;
        let response = reqwest::Client::new().get(url).send().await.unwrap();

        let error = read_body(response, MAX_ICS_SIZE).await.unwrap_err();
        request.await.unwrap();

        assert!(matches!(error, CalendarError::Http(_)));
        assert!(!error.to_string().contains(SECRET));
    }

    #[tokio::test]
    async fn stops_reading_chunked_body_at_size_limit() {
        let (url, request) = serve_response(
            "/calendar.ics",
            concat!(
                "HTTP/1.1 200 OK\r\n",
                "Transfer-Encoding: chunked\r\n",
                "Connection: close\r\n\r\n",
                "4\r\n1234\r\n",
                "5\r\n56789\r\n",
                "0\r\n\r\n",
            )
            .into(),
        )
        .await;
        let response = reqwest::Client::new().get(url).send().await.unwrap();

        let error = read_body(response, 8).await.unwrap_err();
        request.await.unwrap();

        assert!(matches!(error, CalendarError::Parsing(_)));
        assert!(error.to_string().contains("8-byte size limit"));
    }
}
