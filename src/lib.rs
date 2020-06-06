extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

use std::io::prelude::*;

use url::Url;

pub struct Aha {
    pub domain: String,
    pub client: reqwest::Client,
    pub user_email: String,
    pub verbose: bool,
    pub dry_run: bool,
}

impl Aha {
    pub fn url_builder(&self) -> Url {
        let uri = format!("https://{}.aha.io/api/v1/", self.domain);
        Url::parse(&uri).unwrap()
    }

    pub fn status_for_labels(
        &self,
        labels: Vec<String>,
        config_labels: Option<HashMap<String, String>>,
    ) -> Option<String> {
        let mut default_labels = HashMap::new();
        default_labels.insert("In development".to_string(), "In development".to_string());
        default_labels.insert(
            "Needs code review".to_string(),
            "In code review".to_string(),
        );
        default_labels.insert("Needs PM review".to_string(), "In PM review".to_string());
        default_labels.insert("Ready".to_string(), "Ready to ship".to_string());
        labels
            .iter()
            .map(|label| {
                let default = default_labels.get(label);
                let x = match &config_labels {
                    Some(c) => c.get(label).or_else(|| default),
                    None => default,
                };
                match x {
                    Some(c) => Some(c.clone()),
                    None => None,
                }
            })
            .filter(|label| label.is_some())
            .nth(1)
            .unwrap_or(None)
    }

    pub fn new(domain: String, auth_token: String, email: String) -> Aha {
        let mut headers = reqwest::header::HeaderMap::new();
        let mut auth =
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", auth_token)).unwrap();
        auth.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, auth);
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("Rust aha api v1 (Becker@aha.io)"),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        let client = reqwest::Client::builder()
            .gzip(true)
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(50))
            .build()
            .unwrap();
        Aha {
            client,
            domain,
            user_email: email,
            dry_run: false,
            verbose: false,
        }
    }

    pub fn generate_update_function(
        &self,
        current: &Value,
        status: Option<String>,
    ) -> FeatureUpdate {
        let assigned = if current["assigned_to_user"].is_null() {
            Some(self.user_email.clone())
        } else {
            None
        };
        let _count = if current["custom_fields"].is_null() {
            0
        } else {
            current["custom_fields"]
                .as_array()
                .unwrap()
                .iter()
                .by_ref()
                .filter(|cf| cf["name"] == "Pull Request")
                .count()
        };

        let mut status = if let Some(wf) = status {
            Some(WorkflowStatusUpdate { name: wf })
        } else {
            None
        };
        let current_status = &current["workflow_status"]["name"];
        if status.is_none()
            && (current_status == "Ready to develop" || current_status == "Under consideration")
        {
            status = Some(WorkflowStatusUpdate {
                name: "In code review".to_string(),
            })
        }

        FeatureUpdate {
            assigned_to_user: assigned,
            custom_fields: None,
            workflow_status: status,
        }
    }

    pub fn post_aha(
        &self,
        uri: String,
        json_string: Value,
    ) -> Result<Option<Value>, serde_json::Error> {
        if self.verbose {
            println!("puting json: {} | {}", json_string, uri);
        }
        if !self.dry_run {
            let response = self.client.post(&uri).json(&json_string).send();
            let content = response.unwrap().text();
            let text = &content.unwrap_or("".to_string());
            if self.verbose {
                println!("updated {:?}", text);
            }
            let feature: Result<Value, _> = serde_json::from_str(&text);

            if let Ok(f) = feature {
                Ok(Some(f))
            } else {
                if self.verbose {
                    println!("json failed to parse {:?}", text);
                }
                let ex: Result<_, serde_json::Error> = Err(feature.unwrap_err());
                ex
            }
        } else {
            Ok(None)
        }
    }

    pub fn put_aha(
        &self,
        uri: String,
        json_string: Value,
    ) -> Result<Option<Value>, serde_json::Error> {
        if self.verbose {
            println!("puting json: {} | {}", json_string, uri);
        }
        if !self.dry_run {
            let response = self.client.put(&uri).json(&json_string).send();
            let content = response.unwrap().text();
            let text = &content.unwrap_or("".to_string());
            if self.verbose {
                println!("updated {:?}", text);
            }
            let feature: Result<Value, _> = serde_json::from_str(&text);

            if let Ok(f) = feature {
                Ok(Some(f))
            } else {
                if self.verbose {
                    println!("json failed to parse {:?}", text);
                }
                let ex: Result<_, serde_json::Error> = Err(feature.unwrap_err());
                ex
            }
        } else {
            Ok(None)
        }
    }

    pub fn type_from_name(&self, name: &str) -> Option<(String, String)> {
        //could return enum
        let req = Regex::new(r"^([A-Z]+-\d+-\d+)").unwrap();
        let fet = Regex::new(r"^([A-Z]{1,}-\d{1,})").unwrap();
        let rc = req.captures(&name.trim());
        let fc = fet.captures(&name.trim());
        if let Some(rc) = rc {
            Some(("requirement".to_string(), rc[0].to_string()))
        } else if let Some(fc) = fc {
            Some(("feature".to_string(), fc[0].to_string()))
        } else {
            None
        }
    }

    pub fn get(&self, url: Url) -> Result<Value, serde_json::Error> {
        let uri = url.to_string();
        if self.verbose {
            println!(" url: {}", uri);
        }
        let response = self.client.get(&uri).send();
        let content = response.unwrap().text();
        if self.verbose {
            println!("text {:?}", content);
        }
        let feature: Result<Value, _> = serde_json::from_str(&content.unwrap_or("".to_string()));
        if let Ok(fe) = feature {
            Ok(fe)
        } else {
            let ex: Result<Value, serde_json::Error> = Err(feature.unwrap_err());
            ex
        }
    }

    pub fn base_url(&self) -> Url {
        let uri = format!("https://{}.aha.io/api/v1/", self.domain);
        Url::parse(&uri).unwrap()
    }
}

// keep
#[derive(Serialize, Debug, Deserialize)]
pub struct FeatureCreate {
    name: String,
    release_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<CustomNotes>,
}

// keep
#[derive(Serialize, Debug, Deserialize)]
pub struct FeatureUpdateCreate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_fields: Option<CustomFieldGithub>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_status: Option<WorkflowStatusUpdate>,
}

// keep
#[derive(Serialize, Debug, Deserialize)]
pub struct FeatureUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    assigned_to_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_fields: Option<CustomFieldGithub>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_status: Option<WorkflowStatusUpdate>,
}
//keep
#[derive(Serialize, Debug, Deserialize)]
pub struct WorkflowStatusUpdate {
    pub name: String,
}

// kepp
#[derive(Serialize, Debug, Deserialize)]
pub struct CustomNotes {
    #[serde(rename = "release_notes1")]
    notes: String,
}
// kepp
#[derive(Serialize, Debug, Deserialize)]
pub struct CustomFieldGithub {
    #[serde(rename = "pull_request")]
    github_url: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
