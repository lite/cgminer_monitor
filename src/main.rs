extern crate tokio;
extern crate rayon;
extern crate regex;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate chrono;

use std::fs::File;
use std::io::prelude::*;
use chrono::prelude::*;
use regex::Regex;
use std::io::{BufReader, BufWriter, Read};
use std::net::TcpStream;
use std::time::Duration;
use std::path::Path;
use std::process::{Command};

#[derive(Debug, Deserialize, Serialize)]
struct Miner {
    ip: String,
	is_online: bool,
    model: String,
	ghs_5s: String,
	ghs_av: String,
	pool_url: String,
	pool_user: String,
	miner_name: String,
	miner_version: String,
	version: String,
	config: String,
	summary: String,
	pools: String,
	stats: String,
	created_at: String
}

fn info_by_response(buffer: &str) -> Result<(String, String, String, String, String), String>{
	// println!("info_by_response: {}", &buffer);
	let mut version: String = "".to_string();
	let mut config: String =  "".to_string();
	let mut summary: String =  "".to_string();
	let mut pools: String =  "".to_string();
	let mut stats: String =  "".to_string();
	
	let splits: Vec<&str> = buffer.split("CMD=").collect();
	for msg in splits{
		if msg.starts_with("version") { version = msg.to_string() };
		if msg.starts_with("config") { config = msg.to_string() };
		if msg.starts_with("summary") { summary = msg.to_string() };
		if msg.starts_with("pools") { pools = msg.to_string() };
		if msg.starts_with("stats") { stats = msg.to_string() };
	}

	Ok((version.to_string(), config.to_string(), summary.to_string(), pools.to_string(), stats.to_string()))
}

fn get_miner_from_version(version: &str) -> Result<(String, String), String>{
	// println!("get_miner_from_version: {}", &version);
	let re = Regex::new(r"(?m)VERSION,(?P<miner_name>\w+?)=(?P<miner_version>.+?)(,[\w\s]+?=.+?)*$").unwrap();
	let caps = re.captures(&version).unwrap();
	let miner_name = caps["miner_name"].to_string();
	let miner_version = caps["miner_version"].to_string();
	
	Ok((miner_name, miner_version))
}

fn get_config_from_pools(pools: &str) -> Result<(String, String), String>{
	// println!("get_config_from_pools: {}", &pools);
	let re_pools = Regex::new(r"(?m)POOL=0,URL=(?P<pool_url>.+?)(,[\w\s%]+=[\w\s%]+)+,User=(?P<pool_user>.+?),").unwrap();
	let caps_pools = re_pools.captures(&pools).unwrap();

	let pool_url = &caps_pools["pool_url"];
	let pool_user = &caps_pools["pool_user"];

	Ok((pool_url.to_string(), pool_user.to_string()))
}

fn get_model_from_config(config: &str) -> Result<String, String>{
	// println!("get_model_from_config: {}", &config);
	let re_config = Regex::new(r"(?m),Device\sCode=(?P<model>[\w\s]+?)(,.+?=.+?)*$").unwrap();
	let caps_config = re_config.captures(&config).unwrap();

	Ok(caps_config["model"].to_string())
}
fn get_model_from_version(version: &str) -> Result<String, String>{
	// println!("get_model_from_version: {}", &version);
	let re_version = Regex::new(r"(?m)Type=(?P<model>.+?)(,.+?=.+?)*$").unwrap();
	let caps_version = re_version.captures(&version).unwrap();

	Ok(caps_version["model"].to_string())
}

fn get_ghs_from_summary_bmminer(summary: &str) -> Result<(f64, f64), String>{
	// println!("get_ghs_from_summary_bmminer: {}", &summary);
	let re_summary_bm = Regex::new(r"(?m)GHS\s5s=(?P<ghs_5s>.*?),GHS\sav=(?P<ghs_av>.*?)(,[\w\s]+?=.+?)+$").unwrap();
	let caps_summary_bm = re_summary_bm.captures(&summary).unwrap();

	let ghs_5s = caps_summary_bm["ghs_5s"].parse::<f64>().unwrap_or_default();
	let ghs_av = caps_summary_bm["ghs_av"].parse::<f64>().unwrap_or_default();
	
	Ok((ghs_5s, ghs_av))
}

fn get_ghs_from_summary_cgminer(summary: &str) -> Result<(f64, f64), String>{
	// println!("get_ghs_from_summary_cgminer: {}", &summary);
	let re_summary_cg = Regex::new(r"(?m)MHS\sav=(?P<mhs_av>.+?),MHS\s5s=(?P<mhs_5s>.+?)(,[\w\s]+?=.+?)+$").unwrap();
	let caps_summary_cg = re_summary_cg.captures(&summary).unwrap();

	let ghs_5s = caps_summary_cg["mhs_5s"].parse::<f64>().unwrap_or_default() / 1024.0;
	let ghs_av = caps_summary_cg["mhs_av"].parse::<f64>().unwrap_or_default() / 1024.0;
	
	Ok((ghs_5s, ghs_av))
}

fn get_miner_info(ipaddr: &str) -> Result<Miner, String>{
	let host = format!("{0}:{1}", ipaddr, 4028);
	if let Ok(mut stream) = TcpStream::connect(host){
	    println!("Connected to {}!", &ipaddr);
	
		let _result = stream.set_read_timeout(Some(Duration::new(5, 0)));
		let _result = stream.write_all(b"version+config+summary+pools+stats|\n");

		let mut buffer = String::new();
		loop {
		    match stream.read_to_string(&mut buffer) {
		        Ok(_) => break,
		        Err(_e) => {
					return Err("encountered IO error.".to_string());
				}
		    };
		}
		// println!("msg: {:?}", buffer);
		let (version, config, summary, pools, stats) = info_by_response(&buffer).expect("info_by_response error");
		
		let mut model = String::new();
		let mut ghs_5s: f64 = 0.0;
		let mut ghs_av: f64 = 0.0;

		// version
		let (miner_name, miner_version) = get_miner_from_version(&version).expect("get_miner_from_version error");
		println!("miner_name: {} miner_version: {}", &miner_name, &miner_version);	
				
		// pools
		let (pool_url, pool_user) = get_config_from_pools(&pools).expect("get_config_from_pools error");
		println!("pool_url: {} pool_user: {}", &pool_url, &pool_user);	

		match miner_name.as_ref() {
			"CGMiner" => {
				// 10.71.3.11 Device Code=SM CGMiner=4.9.2-git-41ffdeb
				// config
				model = get_model_from_config(&config).expect("get_model_from_config error");
				// let re_config = Regex::new(r"(?m),Device\sCode=(?P<model>[\w\s]+?)(,.+?=.+?)*$").unwrap();
				// let caps_config = re_config.captures(&config).unwrap();
				//
				// model = caps_config["model"].to_string();
			
				// summary
				let _ghs = get_ghs_from_summary_cgminer(&summary).expect("get_ghs_from_summary_cgminer error");
				ghs_5s = _ghs.0;
				ghs_av = _ghs.1;
				
			},
			"BMMiner" => {
				//  10.71.51.14 Type=Antminer T9+
				model = get_model_from_version(&version).expect("get_model_from_version error");
				
				// summary
				let _ghs = get_ghs_from_summary_bmminer(&summary).expect("get_ghs_from_summary_bmminer error");
				ghs_5s = _ghs.0;
				ghs_av = _ghs.1;
			},
			_ =>{}
		}
		println!("model: {} ghs_5s: {} ghs_av: {}", &model, &ghs_5s, &ghs_av);	
		
		let miner = Miner {
		    ip: ipaddr.to_string(),
			is_online: true,
		    model: model.to_string(),
			ghs_5s: ghs_5s.to_string(),
			ghs_av: ghs_av.to_string(),
			pool_url: pool_url.to_string(),
			pool_user: pool_user.to_string(),
			miner_name: miner_name.to_string(),
			miner_version: miner_version.to_string(),
			version: version.to_string(),
			config: config.to_string(),
			summary: summary.to_string(),
			pools: pools.to_string(),
			stats: stats.to_string(),
			created_at: Local::now().to_rfc3339()
		};
		Ok(miner)
	} else {
		Err("Error in connectint to the server...".to_string())
	}
}

/**
fn test_version()
{
	let re_version = Regex::new(r"(?m)\|VERSION,(?P<miner_name>\w+?)=(?P<miner_version>.+?)(,.+?=.+?)*\|").unwrap();
	let version = "STATUS=S,When=1574850226,Code=22,Msg=CGMiner versions,Description=cgminer 4.9.2|VERSION,CGMiner=4.9.2-git-41ffdeb,API=3.7|";
	let caps_version = re_version.captures(&version).unwrap();
	
	println!("miner_name: {}, miner_version: {}", &caps_version["miner_name"], &caps_version["miner_version"]);
}

fn test_config()
{
	let re_config = Regex::new(r"(?m),Device\sCode=(?P<model>[\w\s]+?)(,.+?=.+?)*\|").unwrap();
	let config = "STATUS=S,When=1574851419,Code=33,Msg=CGMiner config,Description=cgminer 4.9.2|CONFIG,ASC Count=3,PGA Count=0,Pool Count=3,Strategy=Failover,Log Interval=5,Device Code=SM ,OS=Linux,Hotplug=None|%";
	let caps_config = re_config.captures(&config).unwrap();
	
	println!("model: {}", &caps_config["model"]);
}

fn test_summary(){
	let re = Regex::new(r"(?m)(?P<key>[\w\s%]+?)=(?P<value>[^,]+),*?").unwrap();
	let string = "STATUS=S,When=1573642564,Code=11,Msg=Summary,Description=bmminer 1.0.0|SUMMARY,Elapsed=108985,GHS 5s=10339.40,GHS av=10347.63,Found Blocks=0,Getworks=15852,Accepted=16219,Rejected=34,Hardware Errors=2895,Utility=8.93,Discarded=59053,Stale=0,Get Failures=137,Local Work=4161408,Remote Failures=0,Network Blocks=174,Total MH=1127725920870.0000,Work Utility=144802.16,Difficulty Accepted=262578176.00000000,Difficulty Rejected=442880.00000000,Difficulty Stale=0.00000000,Best Share=636351451,Device Hardware%=0.0011,Device Rejected%=0.1684,Pool Rejected%=0.1684,Pool Stale%=0.0000,Last getwork=1573642564";
  
	for caps in re.captures_iter(string) {
        println!("key: {}, value: {}", &caps["key"], &caps["value"]);
    }
}

fn test_pools(){
	let re = Regex::new(r"(?m)POOL=0,URL=(?P<pool_url>.+?)(,[\w\s%]+=[\w\s%]+)+,User=(?P<pool_user>.+?),").unwrap();
	let string = "STATUS=S,When=1574756025,Code=7,Msg=3 Pool(s),Description=bmminer 1.0.0|POOL=0,URL=stratum+tcp://61.166.56.36:9778,Status=Alive,Priority=0,Quota=1,Long Poll=N,Getworks=9096,Accepted=15390,Rejected=21,Discarded=87895,Stale=36,Get Failures=68,Remote Failures=2,User=bitmory.001.10x71x51x13,Last Share Time=0:00:42,Diff=32.8K,Diff1 Shares=0,Proxy Type=,Proxy=,Difficulty Accepted=392624139.00000000,Difficulty Rejected=507906.00000000,Difficulty Stale=507904.00000000,Last Share Difficulty=32768.00000000,Has Stratum=true,Stratum Active=true,Stratum URL=61.166.56.36,Has GBT=false,Best Share=2584444801,Pool Rejected%=0.1290,Pool Stale%=0.1290|POOL=1,URL=stratum+tcp://btc.ss.poolin.com:443,Status=Alive,Priority=1,Quota=1,Long Poll=N,Getworks=3317,Accepted=6886,Rejected=10,Discarded=48408,Stale=2,Get Failures=0,Remote Failures=0,User=bitmory.001.10x71x51x13,Last Share Time=3:55:14,Diff=32.8K,Diff1 Shares=0,Proxy Type=,Proxy=,Difficulty Accepted=225640448.00000000,Difficulty Rejected=327680.00000000,Difficulty Stale=0.00000000,Last Share Difficulty=32768.00000000,Has Stratum=true,Stratum Active=false,Stratum URL=,Has GBT=false,Best Share=146776122,Pool Rejected%=0.1450,Pool Stale%=0.0000|POOL=2,URL=stratum+tcp://stratum+tls://btc.ss.poolin.com:5222,Status=Dead,Priority=2,Quota=1,Long Poll=N,Getworks=0,Accepted=0,Rejected=0,Discarded=0,Stale=0,Get Failures=0,Remote Failures=0,User=bitmory.001.10x71x51x13,Last Share Time=0,Diff=,Diff1 Shares=0,Proxy Type=,Proxy=,Difficulty Accepted=0.00000000,Difficulty Rejected=0.00000000,Difficulty Stale=0.00000000,Last Share Difficulty=0.00000000,Has Stratum=true,Stratum Active=false,Stratum URL=,Has GBT=false,Best Share=0,Pool Rejected%=0.0000,Pool Stale%=0.0000";
	let caps = re.captures(&string).unwrap();
	
	println!("pool_url: {}, pool_user:{}", &caps["pool_url"], &caps["pool_user"]);
}

fn test_stats(){
	let re = Regex::new(r"(?m)GHS\s5s=(?P<ghs_5s>.*?),GHS\sav=(?P<ghs_av>.*?)(,[\w\s]+?=.+?)+$").unwrap();
	let string = r"STATUS=S,When=1574905855,Code=11,Msg=Summary,Description=bmminer 1.0.0|SUMMARY,Elapsed=146,GHS 5s=,GHS av=3200.63,Found Blocks=0,Getworks=8,Accepted=4,Rejected=0,Hardware Errors=0,Utility=1.64,Discarded=79,Stale=0,Get Failures=0,Local Work=1747,Remote Failures=0,Network Blocks=1,Total MH=467292234.0000,Work Utility=53865.21,Difficulty Accepted=131072.00000000,Difficulty Rejected=0.00000000,Difficulty Stale=0.00000000,Best Share=188624,Device Hardware%=0.0000,Device Rejected%=0.0000,Pool Rejected%=0.0000,Pool Stale%=0.0000,Last getwork=1574905768";
	let caps = re.captures(&string).unwrap();
	
	println!("ghs_5s:{}, ghs_av: {}", &caps["ghs_5s"].parse::<f64>().unwrap_or_default(), &caps["ghs_av"].parse::<f64>().unwrap_or_default());
}
**/

fn main() {
	// let mut miner_list: Vec<Miner> = Vec::new();
	println!("started at {}", Local::now().to_rfc3339());
	
	let file_name= "ipaddr.txt";
	if !Path::new(file_name).exists(){
		let f = File::create(&file_name).unwrap();
		{
		   	let mut writer = BufWriter::new(f);

			// nmap -sS 20.15.11.2-16 -p 4028
			let output = Command::new("nmap")
			    // .arg("-PS 10.40.116.2-254").arg("-p 4028")
				.arg(r"-v")
				.arg(r"-n")
				// .arg(r"127.0.0.1")
				// .arg(r"10.71.51.12-20")
				// .arg(r"10.71.3.11")
				// .arg(r"10.71.51.14")
				// .arg(r"10.71.7.83")
				// .arg(r"10.71.9.19")
				// .arg(r"10.81.43.124")
				.arg(r"10.81.12.96")
				// .arg(r"10.71.1-99.2-254")
				// .arg(r"10.81.1-99.2-254")
				// .arg(r"10.91.1-99.2-254")
				.arg(r"-PS")
				.arg(r"-p 4028")
				// .arg(r"-p 4028")
			    .output().unwrap();
			
			for line in output.stdout.lines() {
				match line {
			        Ok(msg) => {
						// Nmap scan report for 192.168.2.254 [host down]
						// Discovered open port 8080/tcp on 192.168.1.4
						let re = Regex::new(r"(?m)^Discovered open port (\d+?)/tcp on (?P<ipaddr>[\d.]+?)$").unwrap();
						for caps in re.captures_iter(&msg) {
							let ipaddr = &caps["ipaddr"];
							writer.write_fmt(format_args!("{}\n",ipaddr)).unwrap();
						}
			        },
					Err(_e) => panic!("encountered IO error: {}", _e),
			    };
			}
		}			
	}
	
	// let mut miner_list: Vec<Miner> = Vec::new();
	println!("scan finished at {}", Local::now().to_rfc3339());
	
	let path = "miner_list.csv";
    let mut writer = csv::Writer::from_path(path).unwrap();
	
	let f = File::open(file_name).unwrap();
	{
		let mut reader = BufReader::new(f);
		let mut buffer = String::new();
		
		while reader.read_line(&mut buffer).unwrap() > 0 {
			let ipaddr = buffer.trim();
			println!("ipaddr: {}", ipaddr);
	
			match get_miner_info(&ipaddr) {
		        Ok(miner) => {
					// miner_list.push(miner);
					writer.serialize(miner).expect("CSV writer error");
					writer.flush().expect("Flush error");
				}
		        Err(_e) => {}
		    }

		    buffer.clear();
			// std::thread::sleep( Duration::new(0, 200) );
		}
	}


	println!("ended at {}", Local::now().to_rfc3339());
}

