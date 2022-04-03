use teloxide::{prelude2::*, utils::command::BotCommand};
use std::result::Result;
use reqwest::blocking::Client;
use scraper::Html;
use scraper::Selector;
use scraper::ElementRef;
use std::error::Error;
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/*
    static state variables
*/

static SUBSCRIBERS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    Mutex::new(m)
});

static OBSERVED_SALES: Lazy<Mutex<HashMap<String, Vec<String>>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    Mutex::new(m)
});

/*
    structs:
        -Sale
    enums:
        -Telegram commands 
*/

struct Sale {
    sale_location: Option<String>,
    sale_href: Option<String>,
    sale_price: Option<String>,
}

#[derive(BotCommand, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "Subscribe for flats")]
    Subscribe,
    #[command(description = "Unsubscribe from flats")]
    Unsubscribe,
}

/*
    MAIN
*/

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    log::info!("Starting simple_commands_bot...");
    let bot = Bot::from_env().auto_send();
    teloxide::repls2::commands_repl(bot, answer, Command::ty()).await;
}


/*
    telegram command->response mapping fn
*/

async fn answer(
    bot: AutoSend<Bot>,
    message: Message,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Help => {bot.send_message(message.chat.id, Command::descriptions()).await?;},
        Command::Subscribe => {
            let sales = scrape();
            for sale in sales.into_iter() {
                let location = match sale.sale_location {
                    Some(l) => String::from(l),
                    None => String::from("Unknown location")
                };
                let price = match sale.sale_price {
                    Some(l) => String::from(l),
                    None => String::from("Unknown price")
                };
                let href = match sale.sale_href {
                    Some(l) => String::from(l),
                    None => String::from("Unknown link")
                };
                
                bot.send_message(
                    message.chat.id, 
                    format!("NEW SALE {}:\n\t{}\n{}", location, price, href),
                ).await?;
            }
        },
        Command::Unsubscribe => {bot.send_message(message.chat.id, Command::descriptions()).await?;},
    };
    Ok(())
}

/*
    scraping fns
*/

fn scrape() -> Vec<Sale> {
    let url = "https://www.nepremicnine.net/oglasi-najem/ljubljana-mesto/stanovanje/";
    // let url = "https://www.nepremicnine.net/oglasi-najem/juzna-primorska/stanovanje/";
    
    let mut next_page = true;
    let mut next_page_to_scrape = String::from(url);

    let mut sales = Vec::new();

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

            sales.push(Sale{ 
                sale_location, 
                sale_price, 
                sale_href
            });
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
    sales
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
