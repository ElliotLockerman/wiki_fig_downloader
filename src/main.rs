
use std::path::PathBuf;

use clap::Parser;
use tokio::io::AsyncWriteExt;
use scraper::{Html, Selector};
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;

#[derive(Parser, Debug)]
struct Args {
    // URL to download
    url: String,

    // Directory in which to place images.
    #[arg(short, long, default_value = ".")]
    out: String,
}



lazy_static! {
    static ref IMAGE_SELECTOR: Selector = Selector::parse("div.mw-content-container img").unwrap();
    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}


async fn save_image(mut link: String, mut out: PathBuf) {
    let name = link.rsplit("/").next().unwrap().to_owned();
    out.push(name.clone());

    link.insert_str(0, "http:");

    let res = CLIENT.get(link).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let bytes = res.bytes().await.unwrap();

    let mut file  = tokio::fs::File::create(out).await.unwrap();
    file.write_all(&bytes[..]).await.unwrap();
    println!("{name} done");
}

async fn get_links(url: &str) -> anyhow::Result<impl Iterator<Item=String>> {
    let res = CLIENT.get(url).send().await?;
    assert_eq!(res.status(), 200);
    let body = res.text().await?;

    let html = Html::parse_document(&body);
    let imgs = html.select(&IMAGE_SELECTOR); 
    Ok(
        imgs
            .filter_map(|x| x.attr("src").map(str::to_owned))
            .collect::<Vec<_>>()
            .into_iter()
    )

}

// TODO: get highest resolution available
async fn run(page_url: String, out: PathBuf) {
    let links = match get_links(&page_url).await {
        Ok(x) => x,
        Err(e) => panic!("Error getting links: {e}"),
    };

    let mut stream = FuturesUnordered::from_iter(links)
        .into_iter()
        .map(|x| save_image(x, out.clone()))
        .collect::<FuturesUnordered<_>>();

    while let Some(_) = stream.next().await { }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let dir = PathBuf::from(args.out.as_str());
    if !dir.exists() {
        panic!("Directory '{}' doesn't exist", args.out);
    }

    run(args.url, dir).await;
}
