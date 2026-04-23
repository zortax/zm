use blitz_traits::net::{Bytes, NetHandler, NetProvider, Request};

/// A blocking NetProvider that fetches HTTP(S) URLs and decodes data: URIs.
/// Designed to run on a background thread during document preparation.
pub struct BlockingNetProvider;

impl NetProvider for BlockingNetProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let url = request.url.clone();

        match url.scheme() {
            "data" => {
                if let Ok(data_url) = data_url::DataUrl::process(url.as_str()) {
                    if let Ok((bytes, _)) = data_url.decode_to_vec() {
                        handler.bytes(url.to_string(), Bytes::from(bytes));
                    }
                }
            }
            "https" | "http" => {
                match ureq::get(url.as_str()).call() {
                    Ok(response) => {
                        if let Ok(body) = response.into_body().read_to_vec() {
                            handler.bytes(url.to_string(), Bytes::from(body));
                        }
                    }
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }
}
