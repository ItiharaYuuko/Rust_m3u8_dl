use std::env::args;
use std::fs::{File, write, remove_file, copy};
use std::time::Duration;
use std::io::{BufReader, BufRead, Read};
use std::path::Path;
use std::fs;
use std::os::windows::prelude::*;
use std::collections::HashMap;
use std::process::Command;

use url::Url;
use tokio::task::{yield_now, spawn};
use tokio::sync::{Mutex};
use regex::Regex;
use reqwest::{Client, Error};
use reqwest::header::{HeaderMap, USER_AGENT, HeaderValue};

type ArcMap = Mutex<HashMap<String, String>>;

#[allow(dead_code)]
async fn download_from(f_name: &str, host_name: &str, time_out: u64) -> Result<(), Error> {
    let inf_hm_list = get_m3u8_file_list(f_name, host_name).unwrap();
    let fctx = count_m3u8_ts_files(f_name);
    let mut th_emp = Vec::new();
    let mut th_count = 0;
    for inf_hm in inf_hm_list {
        th_count += 1;
        let th_item = spawn(
            async move {
                {
                    let tmp_hm = inf_hm.lock().await;
                    let dl_result = download_from_url(
                            tmp_hm.get("url").unwrap()
                            , tmp_hm.get("f_name").unwrap()
                            , time_out).await;
                    match dl_result {
                        Ok(_) => {
                            println!("All [{}/{}] Files, <{}> Download successed, {:.2}%."
                            , current_files_count(false)
                            , fctx
                            , tmp_hm.get("f_name").unwrap()
                            , (current_files_count(false) as f32 / fctx as f32 * 100_f32));
                        },
                        Err(_) => {
                            unsized_file_purge(
                                tmp_hm.get("url").unwrap().as_str(),
                                tmp_hm.get("f_name").unwrap().as_str()).await;
                        },
                    }
                }
                yield_now().await;
            });
        th_emp.push(th_item);
    }
    for i in th_emp {
        i.await.unwrap();
    }
    println!("All files and threads count: {}", th_count);
    Ok(())
}

#[allow(dead_code)]
fn get_m3u8_key(f_name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut tmp_vec = vec![];
    let mut key_file = File::open(f_name).expect("Key file open error");
    key_file.read_to_end(&mut tmp_vec).expect("Key file read error.");
    let mut tmp_str = String::new();
    for i in tmp_vec {
        tmp_str += format!("{:x}", i).as_str();
    }
    Ok(tmp_str)
}

#[allow(dead_code)]
async fn decrypt_files(key: &str, m3u8_fn: &str) -> Result<(), Box<dyn std::error::Error>> {
    let vecx = get_m3u8_file_list(m3u8_fn, "").unwrap();
    for inf in vecx {
        let key = format!("{}", key);
        spawn(
            async move {
                {
                    let tmp_inf_hm = inf.lock().await;
                    let cmd = format!("openssl aes-128-cbc -d -in {} -out decrypt\\{} -iv {} -K {}"
                        , tmp_inf_hm.get("f_name").unwrap()
                        , tmp_inf_hm.get("f_name").unwrap()
                        , "0".repeat(32)
                        , key);
                    Command::new("cmd")
                            .arg("/C")
                            .arg(cmd.as_str())
                            .status()
                            .expect("Openssl command executed failed.");
                }
                yield_now().await;
            }
        ).await.unwrap();
    }
    Ok(())
}

#[allow(dead_code)]
fn current_files_count(purge_flg: bool) -> usize {
    let mut file_count: usize = 0;
    for f_path in fs::read_dir(".").unwrap() {
        let tmp_pa = f_path.unwrap().path();
        let tmp_fn = tmp_pa.file_name().unwrap();

        if tmp_fn.to_str().unwrap().ends_with(".ts") ||
             !tmp_fn.to_str().unwrap().contains(".") &&
             tmp_pa.is_file() {
            file_count += 1;
            if purge_flg {
                println!("Removing file <{}> ...", tmp_fn.to_str().unwrap());
                remove_file(tmp_fn).unwrap();
                println!("File <{}> remove successed.", tmp_fn.to_str().unwrap());
            }
        }
    }
    file_count
}

#[allow(dead_code)]
fn count_m3u8_ts_files(f_name: &str) -> usize {
    let mut l_count = 0;
    let rex = Regex::new(r"(^https?\w+)|(^/\w+)|(^[0-9A-za-z]+)").unwrap();
    let m3u8_f = File::open(f_name).unwrap();
    let all_lines = BufReader::new(m3u8_f).lines();
    for line in all_lines {
        if rex.is_match(line.unwrap().as_str()) {
            l_count += 1;
        }
    }
    l_count
}

#[allow(dead_code)]
async fn read_key_url(f_name: &str, host: &str) -> Result<String, Box<dyn std::error::Error>> {
    let rex = Regex::new(r"^#EXT-X-KEY:METHOD=AES-128,URI=").unwrap();
    let m3u8_f = File::open(f_name).unwrap();
    let bf_rd = BufReader::new(m3u8_f);
    let mut tmp_str = String::new();
    for line in bf_rd.lines() {
        if rex.is_match(line.as_ref().unwrap()) {
            let cap_url = line.as_ref()
            .unwrap()
            .split("=")
            .last()
            .unwrap()
            .replace("\"", "");
            if cap_url.starts_with("/") {
                tmp_str = format!("{}{}", host, cap_url);
            } else if cap_url.starts_with("http") {
                tmp_str = format!("{}", cap_url);
            } else {
                tmp_str = format!("{}/{}", host, cap_url);
            }
        }
    }
    Ok(tmp_str)
}

#[allow(dead_code)]
async fn download_from_url(url: &str, f_name: &str, duration: u64) -> Result<(), Error> {
    let url_prefix_flg = Regex::new(r"^https?\w+").unwrap();
    if url_prefix_flg.is_match(url) {
        let url_tmp = Url::parse(url);
        let client_tmp = Client::new();
        let dl_fpath = Path::new(f_name);
        if !dl_fpath.exists() {
            println!("<{}> downloading ...", f_name);
            let mut us_hdm = HeaderMap::new();
            us_hdm.insert(USER_AGENT, HeaderValue::from_str("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:93.0) Gecko/20100101 Firefox/93.0").unwrap());
            let dl_bytes = client_tmp.get(url_tmp.unwrap())
                                                .headers(us_hdm)
                                                .timeout(Duration::from_secs(duration))
                                                .send()
                                                .await;
            match dl_bytes {
                Ok(dl_res) => {
                    let dl_d = dl_res.bytes().await;
                    match dl_d {
                        Ok(dl_data) => {
                            write(dl_fpath, dl_data).unwrap();
                            if f_name.ends_with(".key") {
                                println!("<{}> download successed.", f_name);
                            }
                        },
                        Err(_) => {},
                    }
                },
                Err(_) => {
                },
            }
        } else {
            unsized_file_purge(f_name, url).await;
        }
    }
    Ok(())
}

#[allow(dead_code)]
async fn unsized_file_purge(f_name: &str, url: &str) {
    let url_tmp = Url::parse(url);
    let client_tmp = Client::new();
    let mut us_hdm = HeaderMap::new();
            us_hdm.insert(USER_AGENT, HeaderValue::from_str("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:93.0) Gecko/20100101 Firefox/93.0").unwrap());
    let net_file_size = client_tmp
                        .head(url_tmp.unwrap())
                        .headers(us_hdm)
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await;
    match net_file_size {
        Ok(ns) => {
            let net_size = ns.headers()
                            .get("Content-Length")
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .parse::<u64>()
                            .unwrap();
            let lc_fsize = fs::metadata(f_name).unwrap().file_size();
            if lc_fsize != net_size {
                remove_file(f_name).unwrap();
                print!("{} File pre size = {} not equal to local size = {}, now removed.", f_name, net_size, lc_fsize);
            }
        },
        Err(_) => {}
    }
}

#[allow(dead_code)]
fn get_m3u8_file_list(f_name: &str, host: &str) -> Result<Vec::<ArcMap>, Box<dyn std::error::Error>> {
    let mut f_list: Vec<ArcMap> = Vec::new();
    let rex = Regex::new(r"(^https?\w+)|(^/\w+)|(^[0-9A-za-z]+)").unwrap();
    let m3u8_f = File::open(f_name).unwrap();
    let bf_rd = BufReader::new(m3u8_f);
    for line in bf_rd.lines() {
        if rex.is_match(line.as_ref().unwrap()) {
            let mut full_url = String::from(line.as_ref().unwrap());
            if line.as_ref().unwrap().starts_with("/") {
                full_url = format!("{}{}", host, line.as_ref().unwrap());
            } else if !line.as_ref().unwrap().starts_with("#") {
                if !line.as_ref().unwrap().starts_with("http") {
                    full_url = format!("{}/{}", host, line.as_ref().unwrap());
                }
            } else if line.as_ref().unwrap().starts_with("http") {

            }
            let f_name = String::from(line.as_ref().unwrap().split("/").last().unwrap());
            let mut tmp_info_hm: HashMap<String, String> = HashMap::new();
            tmp_info_hm.insert(String::from("url"), String::from(full_url));
            tmp_info_hm.insert(String::from("f_name"), String::from(f_name));
            f_list.push(Mutex::new(tmp_info_hm));
        }
    }
    Ok(f_list)
}

#[allow(dead_code)]
async fn config_main() -> Result<(), Box<dyn std::error::Error>> {
    let arg_one_two = args().skip(1).collect::<Vec<String>>();
    let mut arg_second = String::new();
    let arg_first = arg_one_two[0].as_str();
    if arg_one_two.len() > 1 {
        arg_second.extend(arg_one_two[1].chars());
    }
    if arg_first.ends_with(".m3u8") {
        let index_m3u8_file_name = String::from("decrypt/") + arg_first;
        if !Path::new(index_m3u8_file_name.as_str()).is_file() {
            copy(arg_first, index_m3u8_file_name).expect("m3u8 file copy to decrypt folder failed.");
        }
        let key_fname = "key.key";
        if !Path::new(key_fname).is_file() {
            let key_url = read_key_url(arg_first, arg_second.as_str()).await.unwrap();
            download_from_url(key_url.as_str(), key_fname, 5).await.unwrap();
        }
        download_from(arg_first, arg_second.as_str(), 60).await.unwrap();
    } else if arg_first.ends_with(".key") {
        let key = get_m3u8_key(arg_first);
        decrypt_files(key.unwrap().as_str(), "index.m3u8").await.unwrap();
    } else if arg_first.starts_with("-") {
        current_files_count(true);
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    config_main().await.unwrap();
}