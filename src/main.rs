
use std::path::PathBuf;

use clap::Parser;
use tokio::io::AsyncWriteExt;
use sxd_xpath::{Value, nodeset::Node};
use sxd_document::Package;

macro_rules! cast {
    ($target: expr, $pat: path) => {
        {
            if let $pat(a) = $target { // #1
                a
            } else {
                panic!(
                    "mismatch variant when cast to {}", 
                    stringify!($pat)); // #2
            }
        }
    };
}


#[derive(Parser, Debug)]
struct Args {
    // URL to download
    url: String,

    // Directory in which to place images.
    #[arg(short, long, default_value = ".")]
    out: String,
}

async fn get_page(page_url: &String) -> Package {
    let res = reqwest::get(page_url).await.unwrap();
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();

    sxd_html::parse_html(body.as_str())
}


 const IMAGE_XPATH: &str = r#"//div[@class="mw-content-container"]//img/@src"#;


async fn save_image(mut link: String, mut out: PathBuf) {
    let name = link.rsplit("/").next().unwrap().to_owned();
    out.push(name.clone());

    link.insert_str(0, "http:");

    let res = reqwest::get(link).await.unwrap();
    assert_eq!(res.status(), 200);
    let bytes = res.bytes().await.unwrap();

    let mut file  = tokio::fs::File::create(out).await.unwrap();
    file.write_all(&bytes[..]).await.unwrap();
    println!("{name} done");
}

// TODO: get highest resolution available
async fn run(page_url: String, out: PathBuf) {
    let page = get_page(&page_url).await;

    let document = page.as_document();
    let value = sxd_xpath::evaluate_xpath(&document, IMAGE_XPATH).unwrap();
    let nodes = cast!(value, Value::Nodeset);
    let jhs: Vec<_> = nodes
        .document_order()
        .iter()
        .map(|node| {
            let link = cast!(node, Node::Attribute).value().to_string();
            let out = out.clone();
            tokio::spawn(async move { save_image(link, out.into()).await })
        })
        .collect();
    
    for jh in jhs {
        jh.await.unwrap();
    }
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
