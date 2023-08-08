use async_trait::async_trait;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, Cursor, Write};
use std::sync::Mutex;
use std::time::{Duration};

use async_recursion::async_recursion;
use lazy_static::lazy_static;
use reqwest::{Client, header, Url};
use scraper::{Html, Selector};
use serde_json::{to_string_pretty, to_value, Value};

lazy_static! {
    static ref DISALLOWED_LINKS: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    static ref VISITED_LINKS_SET: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
    static ref LINKS_BY_PAGE: Mutex<HashMap<String, HashSet<String>>> = Mutex::new(HashMap::new());
    static ref HTTP_CLIENT: Client = reqwest::Client::new();
    static ref A_TAG_SELECTOR: Selector = Selector::parse(A_HTML_TAG).unwrap();
}

const ROBOTS_TXT_PATH: &str = "/robots.txt";
const USER_AGENT: &str = "'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/99.0.4844.83 Safari/537.36'";
const HREF_ATTRIBUTE_NAME: &str = "href";
const A_HTML_TAG: &str = "a";
const REQUEST_TIMEOUT: u64 = 3;

const ALL_LINKS_FILENAME: &str = "all_links.json";
const LINKS_BY_PAGE_FILENAME: &str = "links_by_page.json";

#[async_trait]
pub trait Crawler {
    async fn scrape_site(&self, url_link: String) -> Option<()>;
    fn print_all_links(&self, print_to_file: bool);
    fn print_links_by_page(&self, print_to_file: bool);
}

pub struct WebCrawler;

impl WebCrawler {
    pub fn new() -> Self {
        WebCrawler
    }
}

#[async_trait]
impl Crawler for WebCrawler {
    async fn scrape_site(&self, url_link: String) -> Option<()> {
        process_robots(&url_link).await;
        scrape_page_recursively(url_link).await
    }

    fn print_all_links(&self, print_to_file: bool) {
        print_all_links(print_to_file);
    }

    fn print_links_by_page(&self, print_to_file: bool) {
        print_links_by_page(print_to_file);
    }
}

async fn process_robots(url_link: &String) {
    let robots_link = format!("{}{}", url_link, ROBOTS_TXT_PATH);
    let response_result = HTTP_CLIENT.get(robots_link)
        .header(header::USER_AGENT, USER_AGENT)
        .send()
        .await;

    if let Ok(response) = response_result {
        if let Ok(text_content) = response.text().await {
            let cursor = Cursor::new(text_content);
            let reader = cursor.lines();

            for line in reader {
                if let Ok(parsed_line) = line {
                    if parsed_line.starts_with("Disallow: ") {
                        let path: String = parsed_line["Disallow: ".len()..].to_string();
                        let disallowed_path = strip_to_root_path(path);
                        disallowed_path.map(|disallowed_root|
                            add_to_disallowed_links(disallowed_root));
                    }
                }
            }
        }
    }
}

#[async_recursion]
async fn scrape_page_recursively(link: String) -> Option<()> {
    let html_string_content = fetch_html_content(&link).await?;

    let root_domain = extract_root_domain(&link)?;

    let internal_links = generate_internal_links(html_string_content, &root_domain);

    add_to_links_by_page(link, internal_links.clone());

    let mut thread_handles = Vec::new();

    for internal_link in internal_links.into_iter() {
        let is_link_new_opt = add_to_visited_links(internal_link.clone());

        if let Some(is_link_new) = is_link_new_opt {
            if is_link_new {
                let handle = tokio::spawn(async move {
                    scrape_page_recursively(internal_link).await;
                });
                thread_handles.push(handle);
            }
        }
    }

    for handle in thread_handles {
        handle.await.ok();
    }

    Some(())
}

async fn fetch_html_content(link: &String) -> Option<String> {
    let response_result = HTTP_CLIENT.get(link)
        .header(header::USER_AGENT, USER_AGENT)
        .timeout(Duration::from_secs(REQUEST_TIMEOUT))
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
            eprintln!("Link {} caused the following error: {:?}", link, err);
            None
        }
    };
}

// Trailing slashes are causing unwanted mapping. Prefer a more implicit way to do this.
fn trim_trailing_slash(mut link_to_trim: String) -> String {
    if link_to_trim.ends_with('/') {
        link_to_trim.truncate(link_to_trim.len() - 1)
    }

    link_to_trim
}

fn extract_root_domain(url_string: &String) -> Option<String> {
    let parsed_url = Url::parse(url_string.as_str()).ok()?;
    let base_url = format!("{}://{}", parsed_url.scheme(), parsed_url.domain()?);
    let trimmed_url = trim_trailing_slash(base_url);

    Some(trimmed_url)
}

fn generate_internal_links(html: String, root_domain: &String) -> HashSet<String> {
    let parsed_html = Html::parse_document(html.as_str());

    let mut internal_links = HashSet::new();

    for element in parsed_html.select(&A_TAG_SELECTOR) {
        if let Some(href_value) = element.value().attr(HREF_ATTRIBUTE_NAME) {
            let processed_link_opt = validate_and_process_link(href_value, root_domain);
            processed_link_opt.map(|processed_link| {
                internal_links.insert(processed_link)
            });
        }
    }

    internal_links
}

fn validate_and_process_link(link: &str, root_domain: &String) -> Option<String> {
    let validated_link = validate_link(link, root_domain);
    return validated_link.map(|validated_link| trim_trailing_slash(validated_link));
}

fn validate_link(link: &str, root_domain: &String) -> Option<String> {
    // Assumption: If the link doesn't start with an http/https, it's relative.
    let url_formatted_string = if !link.starts_with("http") && link.starts_with("/") {
        format!("{}{}", root_domain, link)
    } else if link.starts_with("http") {
        link.to_string()
    } else {
        return None;
    };

    let full_url = Url::parse(&url_formatted_string).ok()?;
    let root_url = Url::parse(root_domain).ok()?;

    if full_url.domain()? == root_url.domain()? {
        let path_root = strip_to_root_path(full_url.path().to_string())?;
        let is_disallowed = is_disallowed_link(path_root);

        if !is_disallowed {
            return Some(full_url.to_string());
        }
    }

    None
}

fn strip_to_root_path(link: String) -> Option<String> {
    let mut link_parts = link.split('/').filter(|part| !part.is_empty());

    link_parts.next().map(|first_part| format!("/{}", first_part))
}

fn add_to_disallowed_links(disallowed_path: String) -> Option<bool> {
    DISALLOWED_LINKS
        .lock()
        .map(|mut data| data.insert(disallowed_path))
        .ok()
}

fn is_disallowed_link(prospective_link: String) -> bool {
    DISALLOWED_LINKS
        .lock()
        .map(|data| data.contains(prospective_link.as_str()))
        .unwrap_or(false)
}

fn add_to_visited_links(address: String) -> Option<bool> {
    VISITED_LINKS_SET
        .lock()
        .map(|mut data| data.insert(address))
        .ok()
}

fn add_to_links_by_page(page_link: String, links_in_page: HashSet<String>) {
    LINKS_BY_PAGE
        .lock()
        .map(|mut link_map| link_map.insert(page_link.to_string(), links_in_page))
        .expect("Failed to add value to set.");
}

fn print_all_links(print_to_file: bool) {
    VISITED_LINKS_SET
        .lock()
        .map(|link_set| {
            let json_value: Value = to_value(&*link_set).expect("Failed to convert to JSON");
            let json_string = to_string_pretty(&json_value).expect("Failed to convert to string.");

            if print_to_file {
                let mut file = File::create(ALL_LINKS_FILENAME).expect("Failed to convert to file.");
                file.write_all(json_string.as_bytes()).unwrap();
            } else {
                println!("{}", json_string);
            }
        }).expect("Failed to print all links.");
}

fn print_links_by_page(print_to_file: bool) {
    LINKS_BY_PAGE
        .lock()
        .map(|link_map| {
            let json_value: Value = to_value(&*link_map).expect("Failed to convert to JSON");
            let json_string = to_string_pretty(&json_value).expect("Failed to convert to string.");

            if print_to_file {
                let mut file = File::create(LINKS_BY_PAGE_FILENAME).expect("Failed to convert to file.");
                file.write_all(json_string.as_bytes()).unwrap();
            } else {
                println!("{}", json_string);
            }
        }).expect("Failed to print links by page.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_html_links_total() {
        let html_string = include_str!("../resources/testing_links.html").to_string();
        let root_domain = String::from("https://example.com");

        let internal_links = generate_internal_links(html_string, &root_domain);

        assert_eq!(3, internal_links.len());
    }

    #[test]
    fn test_valid_html_links_relative_link() {
        let html_string = include_str!("../resources/testing_links.html").to_string();
        let root_domain = String::from("https://example.com");

        let internal_links = generate_internal_links(html_string, &root_domain);

        assert_eq!(true, internal_links.contains("https://example.com/goodLink"));
    }

    #[test]
    fn test_valid_html_links_trimmed_link() {
        let html_string = include_str!("../resources/testing_links.html").to_string();
        let root_domain = String::from("https://example.com");

        let internal_links = generate_internal_links(html_string, &root_domain);

        assert_eq!(true, internal_links.contains("https://example.com/goodLinkTrimMe"));
    }

    #[test]
    fn test_valid_html_links_full_link_internal() {
        let html_string = include_str!("../resources/testing_links.html").to_string();
        let root_domain = String::from("https://example.com");

        let internal_links = generate_internal_links(html_string, &root_domain);

        assert_eq!(true, internal_links.contains("https://example.com/goodInternalLink"));
    }

    #[test]
    fn test_valid_html_links_full_link_external() {
        let html_string = include_str!("../resources/testing_links.html").to_string();
        let root_domain = String::from("https://facade.com");

        let internal_links = generate_internal_links(html_string, &root_domain);

        assert_eq!(false, internal_links.contains("https://example.com/goodInternalLink"));
    }
}