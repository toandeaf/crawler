use std::env::args;
use std::time::{Duration, Instant};
use reqwest::Url;
use crate::crawler::{Crawler, WebCrawler};

extern crate lazy_static;

mod crawler;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = args().collect();

    if args.len() < 1 {
        println!("Valid URL required as command line arg");
        return Ok(());
    }

    let target_url_arg = &args[1];
    let target_url = Url::parse(target_url_arg).unwrap().to_string();

    let crawler = WebCrawler::new();

    println!("Starting scrape...");

    let start = Instant::now();
    let _ = crawler.scrape_site(target_url).await;
    let duration: Duration = start.elapsed();

    println!("Time elapsed: {:?}", duration);

    crawler.print_links_by_page(true);
    crawler.print_all_links(true);


    Ok(())
}