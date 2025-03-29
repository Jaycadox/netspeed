use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Location {
    pub city: String,
    pub country: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub url: String,
    pub location: Location,

    #[serde(skip)]
    response: Option<reqwest::Response>,
}

impl Target {
    async fn init(&mut self) -> Result<()> {
        if let None = self.response {
            self.response = Some(reqwest::get(&self.url).await?);
        }

        Ok(())
    }
    pub async fn content_length(&mut self) -> Result<u64> {
        self.init().await?;
        self.response
            .as_ref()
            .unwrap()
            .content_length()
            .ok_or(anyhow!("Unable to get content length"))
    }

    pub fn response(&mut self) -> Option<&mut reqwest::Response> {
        self.response.as_mut()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Client {
    pub ip: String,
    pub asn: String,
    pub isp: String,
    pub location: Location,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse {
    pub client: Client,
    pub targets: Vec<Target>,
}

pub struct Api {
    client: reqwest::Client,
}

impl Api {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_hosts(&self) -> Result<ApiResponse> {
        let url = "https://api.fast.com/netflix/speedtest/v2";
        let params = [
            ("https", "false"),
            ("token", "YXNkZmFzZGxmbnNkYWZoYXNkZmhrYWxm"),
        ];
        let response = self.client.get(url).query(&params).send().await?;
        let response = response.text().await?;
        Ok(serde_json::from_str::<ApiResponse>(&response)?)
    }
}
