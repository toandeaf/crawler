#[macro_use]
extern crate lazy_static;

use std::{collections::BTreeSet, sync::Mutex};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use async_recursion::async_recursion;
use reqwest::{Client, header, Url};
use scraper::{Html, Selector};
use serde_json::{to_string, to_value, Value};

lazy_static! {
    static ref VISITED_LINKS_SET: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    static ref LINKS_BY_PAGE: Mutex<HashMap<String, HashSet<String>>> = Mutex::new(HashMap::new());
    static ref HTTP_CLIENT: Client = reqwest::Client::new();
}

const USER_AGENT: &str = "'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/99.0.4844.83 Safari/537.36'";
const HREF_ATTRIBUTE_NAME: &str = "href";
const A_HTML_TAG: &str = "a";

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // preload_links();

    println!("Starting scrape...");

    let start = Instant::now();
    let _ = main_function_loop("https://monzo.com".to_string()).await;
    let duration: Duration = start.elapsed();

    println!("Time elapsed: {:?}", duration);

    print_links_by_page_json();

    Ok(())
}

// fn preload_links() {
//     add_to_link_set(String::from("/"));
// }

#[async_recursion]
async fn main_function_loop(link: String) -> Option<()> {
    let html_string_content = fetch_html_content(&link).await?;

    let root_domain = extract_root_domain(&link)?;

    let mut thread_handles = Vec::new();

    let internal_links = generate_internal_links(html_string_content, &root_domain);

    add_to_links_by_page(link, internal_links.clone());

    for internal_link in internal_links.into_iter() {
        let is_link_new = add_to_visited_links(internal_link.clone());

        if is_link_new {
            let handle = tokio::spawn(async move {
                main_function_loop(internal_link).await;
            });
            thread_handles.push(handle);
        }
    }

    for handle in thread_handles {
        handle.await.ok();
    }

    Some(())
}

fn validate_and_process_links(link: &str, root_domain: &String) -> Option<String> {
    // If the link doesn't start with an http/https, it's relative.
    if !link.starts_with("http") {
        // Check if this is worth adding
        if link.starts_with("/") && !link.starts_with("/#") {
            return Some(format!("{}{}", root_domain, link));
        }
        return None;
    }

    if let Ok(valid_url) = Url::parse(link) {
        if valid_url.domain().map(|domain| domain == root_domain.as_str()).unwrap_or(false) {
            return Some(link.to_string());
        }
    }
    None
}

fn extract_root_domain(url_string: &String) -> Option<String> {
    return if let Ok(parsed_url) = Url::parse(url_string.as_str()) {
        let mut base_url = format!("{}://{}", parsed_url.scheme(), parsed_url.domain().unwrap());

        if base_url.ends_with('/') {
            base_url.truncate(base_url.len() - 1);
        }
        Some(base_url)
    } else {
        None
    };
}

async fn fetch_html_content(link: &String) -> Option<String> {
    // TODO add file suffix check??
    let response_result = HTTP_CLIENT.get(link)
        .header(header::USER_AGENT, USER_AGENT)
        .timeout(Duration::from_secs(3))
        .send()
        .await;

    return match response_result {
        Ok(response) => {
            if let Some(content_type) = response.headers().get("Content-Type") {
                let content_type_val = content_type.to_str().ok()?;
                if content_type_val == "text/html" {
                    return response.text().await.ok();
                }
            }
            None
        }
        Err(err) => {
            #[cfg(debug_assertions)]
            println!("Link {} caused the following error: {:?}", link, err);
            None
        }
    };
}

fn generate_internal_links(html: String, root_domain: &String) -> HashSet<String> {
    let parsed_html = Html::parse_document(html.as_str());
    let a_selector = Selector::parse(A_HTML_TAG).unwrap();

    let mut internal_links = HashSet::new();

    for element in parsed_html.select(&a_selector) {
        if let Some(href_value) = element.value().attr(HREF_ATTRIBUTE_NAME) {
            let processed_link_opt = validate_and_process_links(href_value, root_domain);
            processed_link_opt.map(|processed_link| internal_links.insert(processed_link));
        }
    }

    internal_links
}

fn add_to_visited_links(address: String) -> bool {
    VISITED_LINKS_SET
        .lock()
        .map(|mut data| data.insert(address))
        .expect("Failed to add value to set.")
}

fn add_to_links_by_page(page_link: String, links_in_page: HashSet<String>) {
    LINKS_BY_PAGE
        .lock()
        .map(|mut link_map| link_map.insert(page_link.to_string(), links_in_page))
        .expect("Failed to add value to set.");
}

fn print_links_by_page_json() {
    LINKS_BY_PAGE.lock().map(|link_map| {
        let json_value: Value = to_value(&*link_map).expect("Failed to convert to JSON");

        let json_string = to_string(&json_value).expect("Failed to convert to string.");

        let mut file = File::create("links_by_page.json").expect("Failed to convert to file.");
        file.write_all(json_string.as_bytes()).unwrap();
    }).expect("TODO: panic message");
}