
use std::path::PathBuf;

use clap::Parser;
use tokio::io::AsyncWriteExt;
use scraper::{Html, Selector};
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use anyhow::{bail, anyhow};

#[derive(Parser, Debug)]
struct Args {
    // URL to download
    url: String,

    // Directory in which to place images.
    #[arg(short, long, default_value = ".")]
    out: String,
}



lazy_static! {
    static ref IMAGE_PAGE_SELECTOR: Selector = 
        Selector::parse("div.mw-content-container a.mw-file-description").unwrap();

    static ref FULL_RES_SELECTOR: Selector = 
        Selector::parse(".fullMedia > p > a.internal").unwrap();

    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}


async fn save_image(mut url: String, mut out: PathBuf) {
    let res: anyhow::Result<()> = (|| async {
        let name = url.rsplit("/").next().ok_or_else(|| anyhow!("No filename"))?.to_owned();
        out.push(name.clone());
        if out.exists() {
            println!("{name} already exists");
            return Ok(());
        }

        url.insert_str(0, "https:");

        let res = CLIENT
            .get(url.clone())
            .header("User-Agent", "TestBot")
            .send().await?;
        if res.status() !=  200 {
            bail!("got status {}", res.status());
        }

        let bytes = res.bytes().await?;

        let mut file  = tokio::fs::File::create(out).await?;
        file.write_all(&bytes[..]).await?;
        println!("{name} done");
        Ok(())
    })().await;

    if let Err(e) = res {
        eprintln!("error downloading '{url}': {e}");
    }
}

async fn download_original(mut url: String, out: PathBuf) {
    let res: anyhow::Result<()> = (|| async {

        // TODO: right language url
        url.insert_str(0, "https://en.wikipedia.org");

        let mut hrefs = get_elem_attrs(&url, &FULL_RES_SELECTOR, "href").await
            .map_err(|e| anyhow!("error finding full-res: {e}"))?;

        match (hrefs.next(), hrefs.next()) {
            (None, None) =>  bail!("No full-resolution images found for {url}"),
            (Some(x), None) => { save_image(x, out).await; Ok(())},
            (Some(x), Some(_)) => {
                save_image(x, out).await;
                bail!("Too many full-resolution images found, ignoring rest");
            },
            (None, Some(_)) => unreachable!(),
        }
    })().await;
    if let Err(e) = res {
        eprintln!("Error with '{url}': {e}");
    }
}

// Skips nodes without the attr
async fn get_elem_attrs(url: &str, selec: &Selector, attr: &str) -> anyhow::Result<impl Iterator<Item=String>> {
    let res = CLIENT.get(url).send().await?;
    if res.status() !=  200 {
        bail!("got status {}", res.status());
    }
    let body = res.text().await?;

    let html = Html::parse_document(&body);
    let imgs = html.select(selec); 
    Ok(
        imgs
            .filter_map(|x| x.attr(attr).map(str::to_owned))
            .collect::<Vec<_>>()
            .into_iter()
    )
}

// TODO: get highest resolution available
async fn run(page_url: String, out: PathBuf) {
    let srcs = match get_elem_attrs(&page_url, &IMAGE_PAGE_SELECTOR, "href").await {
        Ok(x) => x,
        Err(e) => panic!("Error getting links for {page_url}: {e}"),
    };

    let mut stream = FuturesUnordered::from_iter(srcs)
        .into_iter()
        .map(|src| download_original(src, out.clone()))
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
