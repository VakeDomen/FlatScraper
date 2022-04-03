use std::result::Result;
use reqwest::blocking::Client;
use scraper::Html;
use scraper::Selector;
use scraper::ElementRef;

fn main() {
    
    let url = "https://www.nepremicnine.net/oglasi-najem/ljubljana-mesto/stanovanje/";
    // let url = "https://www.nepremicnine.net/oglasi-najem/juzna-primorska/stanovanje/";
    
    let mut next_page = true;
    let mut next_page_to_scrape = String::from(url);

    while next_page {
        let html = fetch_page(next_page_to_scrape.clone());
        
        let selector = Selector::parse(r#"div[itemprop="item"]"#).unwrap();
        for sale in html.select(&selector) {
            
            let sale_location = get_location(sale);
            println!("{:?}", sale_location);
            
            let sale_price = get_price(sale);
            println!("{:?}", sale_price);
            
            let sale_href = get_href(sale);
            println!("{:?}", sale_href);
        }

        // is there a next page?
        next_page = has_next_page(&html);
        if next_page {
            next_page_to_scrape = match get_next_page_href(&html) {
                Some(a) => a,
                None => String::from("")
            };
        }
    }
}

fn get_price(sale: ElementRef) -> Option<String> {
    let pricae_selector = Selector::parse(r#"span[class="cena"]"#).unwrap();
    for price_dom in sale.select(&pricae_selector) {
        return Some(price_dom.inner_html());
    }
    None
}

fn get_href(sale: ElementRef) -> Option<String> {
    let location = Selector::parse(r#"h2[itemprop="name"]"#).unwrap();
    for title_location in sale.select(&location) {
        return match title_location.value().attr("data-href") {
            Some(e) => Some(String::from("https://www.nepremicnine.net") + e),
            None => None
        };
    }
    None
}

fn get_location(sale: ElementRef) -> Option<String> {
    let location = Selector::parse(r#"span[class="title"]"#).unwrap();
    for title_location in sale.select(&location) {
        return Some(title_location.inner_html());
    }
    None
}

fn fetch_page(url: String) -> Html {
    let client = Client::builder().build().unwrap();
    let body_response = get_page_text(
        client, 
        url
    ).unwrap();
    Html::parse_document(&body_response)
}

fn get_page_text(client: Client, url: String) -> Result<String, reqwest::Error> {
    client.get(url).send()?.text()
}

fn get_next_page_href(html: &Html) -> Option<String> {
    let next_page_selector = Selector::parse(r#"a[class="next"]"#).unwrap();
    if has_next_page(html) {
        for next_page_button_ref in html.select(&next_page_selector) {
            return match next_page_button_ref.value().attr("href") {
                Some(e) => Some(String::from("https://www.nepremicnine.net") + e),
                None => None
            };
        }
    }
    None
}

fn has_next_page(html: &Html) -> bool {
    let next_page_selector = Selector::parse(r#"a[class="next"]"#).unwrap();
    let next_page_button_count = html.select(&next_page_selector).count();
    next_page_button_count > 0
}
