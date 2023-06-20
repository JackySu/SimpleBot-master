use crate::plugin::{Action, CommandPlugin, Plugin};
use crate::model::div::{D1PlayerStats, D2PlayerStats, ProfileDTO, StatsDTO, UbiUser};

use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use reqwest::{self, header::HeaderValue};
use thirtyfour::prelude::*;
use serde_json::{from_str, Value};
use std::ops::DerefMut;
use std::sync::Mutex;
use anyhow::anyhow;
use futures::{future::join_all, StreamExt};
use base64::Engine;

use async_trait::async_trait;
use proc_qq::{MessageChainParseTrait, MessageEvent, MessageSendToSourceTrait};
use simple_bot_macros::{action, make_action};

pub struct Div {
    actions: Vec<Box<dyn Action>>,
}

impl Div {
    pub fn new() -> Self {
        
        Self {
            actions: vec![make_action!(tracker)],
        }
    }
}

impl Plugin for Div {
    fn get_name(&self) -> &str {
        "全境查数据"
    }

    fn get_desc(&self) -> &str {
        "查询全境封锁玩家账号数据"
    }
}

#[async_trait]
impl CommandPlugin for Div {
    fn get_actions(&self) -> &Vec<Box<dyn Action>> {
        &self.actions
    }
}

#[action("/div{n} {name}")]
async fn tracker(event: &MessageEvent, n: Option<String>, name: Option<String>) -> anyhow::Result<bool> {
    if n.is_none() || name.is_none() {
        event.send_message_to_source("请输入正确参数 如 /div (1或2) 玩家名".parse_message_chain()).await.unwrap();
        return Ok(false);
    }

    let msg = match n.unwrap().as_str() {
        "1" => {
            let stats = get_div1_player_stats(&name.unwrap()).await?;
            if stats.is_empty() {
                event.send_message_to_source("未找到该玩家".parse_message_chain()).await.unwrap();
                return Ok(false);
            }
            stats.iter().map(|x| x.to_string()).collect::<Vec<String>>().join("\n")
        },
        "2" => { 
            let stats = get_div2_player_stats(&name.unwrap()).await?;
            if stats.is_empty() {
                event.send_message_to_source("未找到该玩家".parse_message_chain()).await.unwrap();
                return Ok(false);
            }
            stats.iter().map(|x| x.to_string()).collect::<Vec<String>>().join("\n")
        },
        _ => "请输入正确的n 应等于1或2".to_string(),
    };

    event
        .send_message_to_source(msg.parse_message_chain())
        .await
        .unwrap();
    Ok(true)
}

// The rest functions are copied from the rust-divtracker-api project

lazy_static! {
    static ref UBI_TICKET: Mutex<String> = Mutex::new("".to_string());
    static ref UBI_SESSION_ID: Mutex<String> = Mutex::new("".to_string());
    static ref UBI_EXPIRATION: Mutex<String> =
        Mutex::new("2015-11-12T00:00:00.0000000Z".to_string());
}

pub async fn check_expiration_date() -> anyhow::Result<()> {
    let expiration = UBI_EXPIRATION.lock().unwrap().clone();
    let mut exp = DateTime::parse_from_rfc3339(&expiration)
        .unwrap()
        .with_timezone(&Utc);
    let mut now = Utc::now() + chrono::Duration::minutes(5);

    let mut login_counts = 0;
    while exp < now && login_counts < 5 {
        login_ubi().await?;
        login_counts += 1;
        println!("Renewed Ubi ticket at {}", now.to_rfc3339());
        let expiration = UBI_EXPIRATION.lock().unwrap().clone();
        exp = DateTime::parse_from_rfc3339(&expiration)
            .unwrap()
            .with_timezone(&Utc);
        now = Utc::now() + chrono::Duration::minutes(5);
    }
    if login_counts >= 5 {
        return Err(anyhow!("Failed to login after 5 trials"));
    }
    Ok(())
}

pub static UBI_LOGIN_URL: &str = "https://public-ubiservices.ubi.com/v3/profiles/sessions";
pub async fn login_ubi() -> anyhow::Result<()> {
    let mut headers = get_common_header();

    let userpass = format!(
        "{}:{}",
        crate::CONFIG.divtrack.ubi_username,
        crate::CONFIG.divtrack.ubi_password
    );
    let mut auth = String::new();
    base64::engine::general_purpose::STANDARD.encode_string(userpass.as_bytes(), &mut auth);
    headers.insert("Authorization", format!("Basic {}", auth).parse().unwrap());
    auth.clear();

    let resp = reqwest::Client::new()
        .post(UBI_LOGIN_URL)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;

    if !resp["errorCode"].is_null() {
        println!("{:#?}", resp);
        return Err(anyhow!("Failed to login to ubi"));
    }

    let mut ticket = UBI_TICKET.lock().unwrap();
    *ticket = resp["ticket"].as_str().unwrap().to_string();

    let mut session_id = UBI_SESSION_ID.lock().unwrap();
    *session_id = resp["sessionId"].as_str().unwrap().to_string();

    let mut expiration = UBI_EXPIRATION.lock().unwrap();
    *expiration = resp["expiration"].as_str().unwrap().to_string();

    Ok(())
}

pub async fn find_player_id_by_db(
    name: &str,
) -> anyhow::Result<Vec<ProfileDTO>> {
    let mut db = crate::RB.lock().await;
    let users = UbiUser::select_by_name(db.deref_mut(), name)
        .await
        .map_err(|e| anyhow!("Failed to find player {} by db\nError: {}", name, e))?;

    let mut profiles = vec![];
    for user in users {
        profiles.push(ProfileDTO { id: user.id, name: None });
    }
    Ok(profiles)
}

pub async fn find_player_id_by_api(
    name: Option<&str>,
    id: Option<&str>
) -> anyhow::Result<Vec<ProfileDTO>> {
    if name.is_none() && id.is_none() {
        return Err(anyhow!("Both name and id are None"));
    }
    if let Err(e) = check_expiration_date().await {
        return Err(anyhow!(e))
    }

    let ticket = UBI_TICKET.lock().unwrap().clone();
    let mut headers = get_common_header();
    headers.insert(
        "Authorization",
        format!("Ubi_v1 t={}", &*ticket).parse().unwrap(),
    );

    let session_id = UBI_SESSION_ID.lock().unwrap().clone();
    headers.insert(
        "Ubi-SessionId",
        (*session_id).parse::<HeaderValue>().unwrap(),
    );

    let mut url = String::from("https://public-ubiservices.ubi.com/v2/profiles?platformType=uplay&");
    if name.is_some() {
        url.push_str(&format!(
            "nameOnPlatform={}",
            name.unwrap()
        ));
    } else {
        url.push_str(&format!("idOnPlatform={}", id.unwrap()));
    }

    let resp = reqwest::Client::new()
        .get(&url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;

    let profiles = &resp["profiles"];
    if profiles.is_array() && profiles.as_array().unwrap().is_empty() {
        return Err(anyhow!("Failed to find player with name: {} id: {} by api", name.clone().unwrap_or(""), id.clone().unwrap_or("")));
    }

    Ok(profiles
        .as_array()
        .unwrap()
        .into_iter()
        .map(|p| ProfileDTO {
            id: p["profileId"].as_str().unwrap().to_string(),
            name: Some(p["nameOnPlatform"].as_str().unwrap().to_string()),
        })
        .collect::<Vec<ProfileDTO>>())
}

pub async fn get_player_profiles_by_name(
    name: &str,
) -> anyhow::Result<Vec<ProfileDTO>> {
    let mut profiles = find_player_id_by_api(Some(name), None).await.unwrap_or(vec![]);
    if profiles.is_empty() {
        profiles = find_player_id_by_db(name).await?;
    }
    Ok(profiles)
}

pub async fn get_player_stats_by_name(
    name: &str,
    game_space_id: &str,
) -> anyhow::Result<Vec<StatsDTO>> {
    if let Err(e) = check_expiration_date().await {
        return Err(anyhow!(e))
    }

    let mut headers = get_common_header();
    let ticket = UBI_TICKET.lock().unwrap().clone();
    headers.insert(
        "Authorization",
        format!("Ubi_v1 t={}", &ticket).parse().unwrap(),
    );

    let session_id = UBI_SESSION_ID.lock().unwrap().clone();
    headers.insert(
        "Ubi-SessionId",
        (*session_id).parse::<HeaderValue>().unwrap(),
    );

    let mut profiles = get_player_profiles_by_name(name).await?;

    let mut results: Vec<StatsDTO> = vec![];
    let urls = profiles
        .iter()
        .map(|p| {
            let url = format!(
                "https://public-ubiservices.ubi.com/v1/profiles/{}/statscard?spaceId={}",
                p.id, game_space_id
            );
            url
        })
        .collect::<Vec<String>>();

    let client = reqwest::Client::new();
    let stream =
        futures::stream::iter(urls).map(|url| client.get(&url).headers(headers.clone()).send());

    let mut stream = stream.buffered(5);

    let mut i = 0;
    while let Some(result) = stream.next().await {
        let resp = result?.json::<Value>().await?;
        if !resp["errorCode"].is_null() {
            println!("{:#?}", resp);
            return Err(anyhow!("Failed to get stats for user {}", &profiles[i].id));
        }
        let profile = &mut profiles[i];
        match UbiUser::store_user_name(&profile.id, &profile.name.clone().unwrap_or("?".to_string())).await {
            Ok(_) => println!("Created or update user {}", &profile.id),
            Err(e) => {
                println!("Failed to create or update user {}: {:?}", &profile.id, e)
            }
        }
        let name = match &profile.name {
            Some(n) => (*n).clone(),
            None => {
                println!("Failed to get name for user {}", &profile.id);
                let url = format!(
                    "https://public-ubiservices.ubi.com/v2/profiles?userId={}&platformType=uplay",
                    &profile.id
                );
                let res = reqwest::Client::new()
                    .get(&url)
                    .headers(headers.clone())
                    .send()
                    .await?
                    .json::<Value>()
                        .await?["profiles"][0]["nameOnPlatform"]
                        .as_str()
                        .unwrap()
                        .to_string();
                profile.name = Some(res.clone());
                res
            }
        };

        match UbiUser::store_user_name(&profile.id, &name).await {
            Ok(_) => println!("Stored name {} for user {}", &name, &profile.id),
            Err(e) => {
                println!(
                    "Failed to store name {} for user {}: {:?}",
                    &name, &profile.id, e
                );
            }
        }
        results.push(StatsDTO {
            stats: resp["Statscards"].as_array().unwrap().clone(),
            profile: profile.clone(),
        });
        i += 1;
    }

    if results.is_empty() {
        return Err(anyhow!("Failed to find player {} by either api or db", name));
    }
    Ok(results)
}

pub static DIV1_SPACE_ID: &str = "6edd234a-abff-4e90-9aab-b9b9c6e49ff7";
pub async fn get_div1_player_stats(
    name: &str,
) -> anyhow::Result<Vec<D1PlayerStats>> {
    let res = get_player_stats_by_name(name, DIV1_SPACE_ID).await?;
    Ok(join_all(
        res.into_iter()
            .map(|r| async move {
                let p = r.profile;
                let s = r.stats;
                D1PlayerStats {
                    id: p.id.clone(),
                    name: p.name.unwrap_or("".to_string()),
                    level: s[0]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    dz_rank: s[1]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    ug_rank: s[2]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    playtime: s[3]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0) / 3600,
                    main_story: s[4]["value"].as_str().unwrap_or("0 %").to_string(),
                    rogue_kills: s[5]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    items_extracted: s[6]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    skill_kills: s[7]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    total_kills: s[8]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    gear_score: s[11]["value"].as_str().unwrap().parse::<u64>().unwrap_or(0),
                    all_names: UbiUser::get_user_names_by_id(p.id.clone().as_str())
                        .await
                        .unwrap_or(vec![]),
                }
            })
            .collect::<Vec<_>>(),
    )
    .await)
}

// pub static DIV2_SPACE_ID: &str = "60859c37-949d-49e2-8fc8-6d8dc40f1a9e";
pub static TRACKER_URL: &str = "https://api.tracker.gg/api/v2/division-2/standard/profile/uplay/";
pub async fn get_div2_player_stats(
    
    name: &str,
) -> anyhow::Result<Vec<D2PlayerStats>> {
    let mut profiles = find_player_id_by_api(Some(name), None).await.unwrap_or(vec![]);

    if profiles.is_empty() {
        profiles = find_player_id_by_db(&name).await?;
        if profiles.is_empty() {
            return Err(anyhow!("Failed to find player {} by either api or db", name));
        }
        for profile in profiles.iter_mut() {
            profile.name = find_player_id_by_api(None, Some(&profile.id)).await?[0].name.clone();
        }

    } else {
        match UbiUser::store_user_name(&profiles[0].id, &profiles[0].name.clone().unwrap_or("?".to_string())).await {
            Ok(_) => println!("Created or update user {}", &profiles[0].id),
            Err(e) => {
                println!("Failed to create or update user {}: {:?}", &profiles[0].id, e)
            }
        }
    }

    let p = &profiles[0];
    let p_name = p.name.clone().unwrap_or("".to_string());
    match UbiUser::store_user_name(&p.id, &p_name).await {
        Ok(_) => println!("Stored name {} for user {}", &p_name, &p.id),
        Err(e) => {
            println!(
                "Failed to store name {} for user {}: {:?}",
                &name, &p.id, e
            );
        }
    }

    let driver = get_webdriver().await?;
    driver.goto(format!("{}{}", TRACKER_URL, p.name.clone().unwrap_or("".to_string()))).await.unwrap();
    let data = driver.find(By::Css("body")).await?.text().await?;
    driver.quit().await?;

    let metadata: Value = from_str(&data)?;
    let stats = &metadata["data"]["segments"][0]["stats"];
    
    if stats.is_null() {
        return Err(anyhow!("player {} exists but no profile for this game", name));
    }
    Ok(vec![D2PlayerStats {
        id: p.id.clone(),
        name: p.name.clone().unwrap_or("".to_string()),
        total_playtime: stats["timePlayed"]["value"].as_u64().unwrap_or(0) / 3600,
        level: stats["highestPlayerLevel"]["value"].as_u64().unwrap_or(0),
        pvp_kills: stats["killsPvP"]["value"].as_u64().unwrap_or(0),
        npc_kills: stats["killsNpc"]["value"].as_u64().unwrap_or(0),
        headshots: stats["headshots"]["value"].as_u64().unwrap_or(0),
        headshot_kills: stats["killsHeadshot"]["value"].as_u64().unwrap_or(0),
        shotgun_kills: stats["killsWeaponShotgun"]["value"].as_u64().unwrap_or(0),
        smg_kills: stats["killsWeaponSubMachinegun"]["value"].as_u64().unwrap_or(0),
        pistol_kills: stats["killsWeaponPistol"]["value"].as_u64().unwrap_or(0),
        rifle_kills: stats["killsWeaponRifle"]["value"].as_u64().unwrap_or(0),
        player_kills: stats["playersKilled"]["value"].as_u64().unwrap_or(0),
        xp_total: stats["xPTotal"]["value"].as_u64().unwrap_or(0),
        pve_xp: stats["xPPve"]["value"].as_u64().unwrap_or(0),
        pvp_xp: stats["xPPvp"]["value"].as_u64().unwrap_or(0),
        clan_xp: stats["xPClan"]["value"].as_u64().unwrap_or(0),
        sharpshooter_kills: stats["killsSpecializationSharpshooter"]["value"].as_u64().unwrap_or(0),
        survivalist_kills: stats["killsSpecializationSurvivalist"]["value"].as_u64().unwrap_or(0),
        demolitionist_kills: stats["killsSpecializationDemolitionist"]["value"].as_u64().unwrap_or(0),
        e_credit: stats["eCreditBalance"]["value"].as_u64().unwrap_or(0),
        commendation_count: stats["commendationCount"]["value"].as_u64().unwrap_or(0),
        commendation_score: stats["commendationScore"]["value"].as_u64().unwrap_or(0),
        gear_score: stats["latestGearScore"]["value"].as_u64().unwrap_or(0),
        dz_rank: stats["rankDZ"]["value"].as_u64().unwrap_or(0),
        dz_playtime: stats["timePlayedDarkZone"]["value"].as_u64().unwrap_or(0) / 3600,
        rogues_killed: stats["roguesKilled"]["value"].as_u64().unwrap_or(0),
        rogue_playtime: stats["timePlayedRogue"]["value"].as_u64().unwrap_or(0) / 3600,
        longest_rogue: stats["timePlayedRogueLongest"]["value"].as_u64().unwrap_or(0) / 60,
        conflict_rank: stats["latestConflictRank"]["value"].as_u64().unwrap_or(0),
        conflict_playtime: stats["timePlayedConflict"]["value"].as_u64().unwrap_or(0) / 3600,
        all_names: UbiUser::get_user_names_by_id(profiles[0].id.clone().as_str())
            .await
            .unwrap_or(vec![])
    }])
}

use reqwest::header;

pub static USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36";
pub static CONTENT_TYPE: &str = "application/json; charset=utf-8";
pub static ACCEPT: &str = "application/json, text/plain, */*";
pub static REQUEST_PLATFORM_TYPE: &str = "uplay";
pub static REQUEST_WITH: &str = "XMLHttpRequest";
pub static CACHE_CONTROL: &str = "no-cache";
pub static LOCALE: &str = "en-US";
pub static REFERER: &str = "https://connect.ubisoft.com";
pub static HOST: &str = "public-ubiservices.ubi.com";
pub static ENCODING: &str = "gzip, deflate, br";
pub static UBI_LOCALE_CODE: &str = "en-US";
pub static UBI_APPID: &str = "314d4fef-e568-454a-ae06-43e3bece12a6";
pub fn get_common_header() -> header::HeaderMap {
    let mut headers = header::HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, CONTENT_TYPE.parse().unwrap());
    headers.insert(header::USER_AGENT, USER_AGENT.parse().unwrap());
    headers.insert(header::ACCEPT, ACCEPT.parse().unwrap());
    headers.insert(header::HOST, HOST.parse().unwrap());
    headers.insert(header::CACHE_CONTROL, CACHE_CONTROL.parse().unwrap());
    headers.insert(header::ACCEPT_LANGUAGE, LOCALE.parse().unwrap());
    headers.insert(header::ACCEPT_ENCODING, ENCODING.parse().unwrap());
    headers.insert(header::REFERER, REFERER.parse().unwrap());
    headers.insert(header::ORIGIN, REFERER.parse().unwrap());
    headers.insert("Ubi-AppId", UBI_APPID.parse().unwrap());
    headers.insert("Ubi-RequestedPlatformType", REQUEST_PLATFORM_TYPE.parse().unwrap());
    headers.insert("Ubi-LocaleCode", UBI_LOCALE_CODE.parse().unwrap());
    headers.insert("X-Requested-With", REQUEST_WITH.parse().unwrap());
    headers
}

pub async fn get_webdriver() -> WebDriverResult<WebDriver> {
    let mut caps = DesiredCapabilities::chrome();

    let _ = caps.set_disable_web_security();
    let _ = caps.add_chrome_arg("--ssl-protocol=any");
    let _ = caps.add_chrome_arg("--ignore-ssl-errors=true");
    let _ = caps.add_chrome_arg("--disable-extensions");
    let _ = caps.add_chrome_arg("start-maximized");
    let _ = caps.add_chrome_arg("window-size=1280,720");
    let _ = caps.add_chrome_arg("disable-infobars");
    let _ = caps.add_chrome_option("detach", true);

    let port = crate::CONFIG.divtrack.chrome_port;
    let driver = WebDriver::new(format!("{}{}", "http://localhost:", port).as_str(), caps).await?;
    Ok(driver)
}