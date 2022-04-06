use teloxide::{prelude2::*, utils::command::BotCommand };
use std::result::Result;
use reqwest::blocking::Client;
use scraper::Html;
use scraper::Selector;
use scraper::ElementRef;
use std::error::Error;
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use tokio_cron_scheduler::{JobScheduler, Job};
use std::thread;
use std::env;
use dotenv::dotenv;

static SUBSCRIBERS: Lazy<Mutex<HashMap<i64, Vec<String>>>> = Lazy::new(|| {
    match serde_any::from_file("subscribers.json") {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

static OBSERVED_SALES: Lazy<Mutex<HashMap<i64, Vec<String>>>> = Lazy::new(|| {
    match serde_any::from_file("sales.json") {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

static FIRST_SCRAPES: Lazy<Mutex<HashMap<i64, Vec<String>>>> = Lazy::new(|| {
    match serde_any::from_file("first_scrapes.json") {
        Ok(hm) => Mutex::new(hm),
        Err(_) => Mutex::new(HashMap::new())
    }
});

#[derive(Clone, Debug)]
struct Sale {
    sale_id: Option<String>,
    sale_location: Option<String>,
    sale_href: Option<String>,
    sale_price: Option<String>,
    sale_size: Option<String>,
}

#[derive(BotCommand, Clone)]
#[command(rename = "lowercase", description = "These commands are supported:")]
enum Command {
    #[command(description = "display this text.")]
    Help,
    #[command(description = "Subscribe for flats")]
    Subscribe(String),
    #[command(description = "Unsubscribe from flats")]
    Unsubscribe(String),
    #[command(description = "List all subscriptions")]
    List,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let token = env::var("TELEGRAM_BOT_TOKEN").expect("$TELEGRAM_BOT_TOKEN is not set");
    env::set_var("TELOXIDE_TOKEN", token);
    pretty_env_logger::init();
    let bot = Bot::from_env().auto_send();
    thread::spawn(|| {
        run_cron();
    });
    println!("Running telegram bot!");
    teloxide::repls2::commands_repl(bot, answer, Command::ty()).await;
}

#[tokio::main]
async fn run_cron() {
    let mut sched = JobScheduler::new();
    match sched.add(Job::new_async("0 10,20,30,40,50,0 * * * *", move |_, _|  Box::pin(async { 
        match scrape().await {
            Ok(_) => (),
            Err(e) => println!("{:?}", e)
        }
    })).unwrap()) {
        Ok(c) => println!("Started cron!: {:?}", c),
        Err(e) => println!("Something went wrong scheduling CRON: {:?}", e)
    };
    match sched.set_shutdown_handler(Box::new(|| {
        Box::pin(async move {
          println!("Shut down done");
        })
    })) {
        Ok(c) => println!("Shutdown handler set for cron!: {:?}", c),
        Err(e) => println!("Something went wrong setting shutdown handler for CRON: {:?}", e)
    };
    if let Err(e) = sched.start().await {
        eprintln!("Error on scheduler {:?}", e);
    }
}

async fn answer(
    bot: AutoSend<Bot>,
    message: Message,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Help => { bot.send_message(message.chat.id, Command::descriptions()).await? },
        Command::Unsubscribe(url) => { bot.send_message(message.chat.id, unsubscribe(&bot, message, url)).await? },
        Command::Subscribe(url) => { bot.send_message(message.chat.id, subscribe(&bot, message, url)).await? },
        Command::List => { bot.send_message(message.chat.id, list_subscritions(&bot, message)).await? },
    };
    Ok(())
}

fn list_subscritions(
    _: &AutoSend<Bot>,
    message: Message,
)  -> String  {
    let mut subs = SUBSCRIBERS.lock().unwrap();
    match subs.get_mut(&message.chat.id) {
        Some(v) =>  v.join("\n"),
        None => format!("Not subbed to anything..."),
    }
}

fn unsubscribe(
    _: &AutoSend<Bot>,
    message: Message,
    url: String,
) -> String {
    let mut subs = SUBSCRIBERS.lock().unwrap();
    let resp = match subs.get_mut(&message.chat.id) {
        Some(v) =>  {
            if v.iter().find(|&x| *x == *url) != None {
                let index = v.iter().position(|x| *x == *url).unwrap();
                v.remove(index);
                println!("Removed subscription: {:?}", url);
                println!("New state: {:?}", subs);
                format!("Successfully unsubed from link.")
            } else {
                println!("Sub does not exist: {:?}", subs);
                format!("Not subed to that link.")
            }
        },
        None => format!("Not subbed to anything..."),
    };
    match serde_any::to_file("subscribers.json", &*subs) {
        Ok(_) => {();},
        Err(e) => {println!("Error saving subscirbers: {:?}", e);}
    };
    resp
}

fn subscribe(
    _: &AutoSend<Bot>,
    message: Message,
    url: String,
) -> String {
    let mut subs = SUBSCRIBERS.lock().unwrap();
    subs.entry(message.chat.id).or_insert(Vec::new());
    let resp = match subs.get_mut(&message.chat.id) {
        Some(v) =>  {
            if v.iter().find(|&x| *x == *url) == None {
                v.push(url);
                println!("New subscription: {:?}", subs);
                format!("Successfully subed to link.")
            } else {
                println!("Existing subscription: {:?}", subs);
                format!("Already subed to link.")
            }
        },
        None => format!("Something is not right..."),
    };
    match serde_any::to_file("subscribers.json", &*subs) {
        Ok(_) => {();},
        Err(e) => {println!("Error saving subscirbers: {:?}", e);}
    };
    resp
}

async fn scrape() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Scraping!");
    let subs = SUBSCRIBERS.lock().unwrap();
    let sales = OBSERVED_SALES.lock().unwrap();
    let mut scrapes = FIRST_SCRAPES.lock().unwrap();
    for (subscriber, jobs) in &*subs {
        scrapes.entry(*subscriber).or_insert(Vec::new());
        for job in jobs {            
            let sales = scrape_url(job);
            let notification_sales = filter_to_notify(subscriber, sales);
            let notify = match scrapes.get_mut(subscriber) {
                Some(v) => {
                    if v.iter().find(|&x| *x == *job) != None {
                        true
                    } else {
                        v.push(job.clone());
                        false
                    }
                },
                None => false
            };
            if !notify {
                continue;
            }
            for sale in notification_sales {
                let sub_id = *subscriber;
                tokio::task::spawn(async move {
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
                    let size = match sale.sale_size {
                        Some(l) => String::from(l),
                        None => String::from("Unknown size")
                    };    
                    match Bot::from_env().auto_send().send_message(
                        sub_id,
                        format!("{}:\n\t{}\n\t{}\n{}", location, price, size, href)
                    ).await {
                        Ok(e) => println!("{:?}", e),
                        Err(e) => println!("{:?}", e),
                    };
                });
            }
        }
    }
    match serde_any::to_file("sales.json", &*sales) {
        Ok(_) => (),
        Err(e) => println!("Error saving subscirbers: {:?}", e)
    };
    match serde_any::to_file("first_scrapes.json", &*scrapes) {
        Ok(_) => (),
        Err(e) => println!("Error saving subscirbers: {:?}", e)
    };
    Ok(())
}

fn filter_to_notify(subscriber: &i64, sales: Vec<Sale>) -> Vec<Sale> {
    let mut sales_to_notify: Vec<Sale> = Vec::new();
    let mut seen = OBSERVED_SALES.lock().unwrap();
    let sales_ids: Vec<String> = sales.iter().map(|sale| {
        match &sale.sale_id {
            Some(id) => String::from(id),
            None => String::from("missing")
        }
    }).collect();
    match seen.get_mut(subscriber) {
        Some(seen_by_sub) => {
            for sale in sales {
                let sale_id = match &sale.sale_id {
                    Some(id) => String::from(id),
                    None => String::from("missing")
                };
                if !seen_by_sub.contains(&sale_id) {
                    sales_to_notify.push(sale);
                    seen_by_sub.push(sale_id);
                }
            }
        },
        None => {
            seen.insert(*subscriber, sales_ids);
        },
    }
    sales_to_notify
}

fn scrape_url(url: &str) -> Vec<Sale> {
    let mut next_page = true;
    let mut next_page_to_scrape = String::from(url);
    let mut sales = Vec::new();
    while next_page {
        let html = fetch_page(next_page_to_scrape.clone());
        let selector = Selector::parse(r#"div[itemprop="item"]"#).unwrap();
        for sale in html.select(&selector) {
            let sale_id = get_id(sale);
            let sale_location = get_location(sale);
            let sale_price = get_price(sale);
            let sale_href = get_href(sale);
            let sale_size = get_size(sale);
            sales.push(Sale{ 
                sale_id,
                sale_location, 
                sale_price, 
                sale_href,
                sale_size,
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
    let price_selector = Selector::parse(r#"span[class="cena"]"#).unwrap();
    for price_dom in sale.select(&price_selector) {
        return Some(price_dom.inner_html());
    }
    None
}

fn get_href(sale: ElementRef) -> Option<String> {
    let href_selector = Selector::parse(r#"h2[itemprop="name"]"#).unwrap();
    for href_dom in sale.select(&href_selector) {
        return match href_dom.value().attr("data-href") {
            Some(e) => Some(String::from("https://www.nepremicnine.net") + e),
            None => None
        };
    }
    None
}

fn get_location(sale: ElementRef) -> Option<String> {
    let location_selector = Selector::parse(r#"span[class="title"]"#).unwrap();
    for location_dom in sale.select(&location_selector) {
        return Some(location_dom.inner_html());
    }
    None
}

fn get_size(sale: ElementRef) -> Option<String> {
    let size_selector = Selector::parse(r#"span[class="velikost"]"#).unwrap();
    for size_dom in sale.select(&size_selector) {
        return Some(size_dom.inner_html());
    }
    None
}

fn get_id(sale: ElementRef) -> Option<String> {
    let id_selector = Selector::parse(r#"h2[itemprop="name"]"#).unwrap();
    for id_containing_dom in sale.select(&id_selector) {
        return match id_containing_dom.value().attr("data-href") {
            Some(e) => {
                let split = e.split("_");
                match split.last() {
                    Some(s) => Some(String::from(s)),
                    None => None
                }
            },
            None => None
        };
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
