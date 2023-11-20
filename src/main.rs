use std::path::PathBuf;

use anyhow::{anyhow, bail};
use clap::Parser;
use futures::stream::{FuturesUnordered, StreamExt};
use lazy_static::lazy_static;
use scraper::{Html, Selector};
use tokio::io::AsyncWriteExt;

#[derive(Parser, Debug)]
struct Args {
    // URL to download
    url: String,

    // Directory in which to place images.
    #[arg(short, long, default_value = ".")]
    out: String,
}

lazy_static! {
    // Selects all <a> tags around images.
    static ref IMAGE_LINK_SELECTOR: Selector =
        Selector::parse("figure a.mw-file-description").unwrap();

    // Selects the "Original file" link on an image's page.
    static ref ORIGINAL_SELECTOR: Selector =
        Selector::parse(".fullMedia > p > a.internal").unwrap();

    static ref CLIENT: reqwest::Client = reqwest::Client::new();
}

async fn save_image(mut url: String, mut out: PathBuf) -> anyhow::Result<()> {
    let name = url
        .rsplit("/")
        .next()
        .ok_or_else(|| anyhow!("No filename"))?
        .to_owned();
    out.push(name.clone());
    if out.exists() {
        println!("{name} already exists");
        return Ok(());
    }

    url.insert_str(0, "https:");

    let res = CLIENT
        .get(url.clone())
        .header("User-Agent", "TestBot")
        .send()
        .await?;
    if res.status() != 200 {
        bail!("got status {}", res.status());
    }

    let bytes = res.bytes().await?;

    let mut file = tokio::fs::File::create(out).await?;
    file.write_all(&bytes[..]).await?;
    println!("{name} done");
    Ok(())
}

async fn download_original(mut url: String, out: PathBuf) {
    let res: anyhow::Result<()> = (|| async {
        // TODO: right language url based on original link
        url.insert_str(0, "https://en.wikipedia.org");

        let mut hrefs = get_elem_attrs(&url, &ORIGINAL_SELECTOR, "href")
            .await
            .map_err(|e| anyhow!("error finding full-res: {e}"))?;

        let link = hrefs.next().ok_or(anyhow!("No full-resolution images found"))?;
        save_image(link, out).await?;

        if let Some(_) = hrefs.next() {
            eprintln!("Warning: multiple originals found for {url}, ignoring rest");
        }

        Ok(())
    })()
    .await;
    if let Err(e) = res {
        eprintln!("Error with '{url}': {e}");
    }
}

// Skips nodes without the attr
async fn get_elem_attrs(
    url: &str,
    selec: &Selector,
    attr: &str,
) -> anyhow::Result<impl Iterator<Item = String>> {
    let res = CLIENT.get(url).send().await?;
    if res.status() != 200 {
        bail!("got status {}", res.status());
    }
    let body = res.text().await?;

    let html = Html::parse_document(&body);
    let imgs = html.select(selec);
    Ok(imgs
        .filter_map(|x| x.attr(attr).map(str::to_owned))
        .collect::<Vec<_>>()
        .into_iter())
}

// TODO: get highest resolution available
async fn run(page_url: String, out: PathBuf) {
    let srcs = match get_elem_attrs(&page_url, &IMAGE_LINK_SELECTOR, "href").await {
        Ok(x) => x,
        Err(e) => panic!("Error getting links for {page_url}: {e}"),
    };

    // NB: this allows unlimited concurrency, which may be undesirable.
    let mut stream = FuturesUnordered::from_iter(srcs)
        .into_iter()
        .map(|src| download_original(src, out.clone()))
        .collect::<FuturesUnordered<_>>();

    while let Some(_) = stream.next().await {}
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
