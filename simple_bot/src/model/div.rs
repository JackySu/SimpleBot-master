use crate::RB;
use rbatis::{crud, impl_select};
use rbatis::rbdc::datetime::DateTime;
use serde_json::Value;
use serde::{Serialize, Deserialize};
use std::fmt::{Display, Formatter};
use std::ops::DerefMut;

#[derive(Debug, Serialize, Deserialize)]
pub struct D1PlayerStats {
    #[serde(skip_serializing)]
    pub id: String,
    pub name: String,
    pub level: u64,
    pub dz_rank: u64,
    pub ug_rank: u64,
    pub playtime: u64,
    pub main_story: String,
    pub total_kills: u64,
    pub rogue_kills: u64,
    pub items_extracted: u64,
    pub skill_kills: u64,
    pub gear_score: u64,
    pub all_names: Vec<String>,
}

impl Display for D1PlayerStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "玩家名：{}\n玩家等级: {}\n暗区等级: {}\n地下等级: {}\n游戏时间: {}\n主线任务：{}\nNPC击杀: {}\n红名击杀: {}\n技能击杀: {}\n回收物品: {}\n装等分数: {}\n所有名字：{:?}\n",
            self.name, self.level, self.dz_rank, self.ug_rank, self.playtime, self.main_story, self.total_kills, self.rogue_kills, self.skill_kills, self.items_extracted, self.gear_score, self.all_names)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct D2PlayerStats {
    #[serde(skip_serializing)]
    pub id: String,
    pub name: String,
    pub total_playtime: u64,
    pub level: u64,
    pub pvp_kills: u64,
    pub npc_kills: u64,
    pub headshots: u64,
    pub headshot_kills: u64,
    pub shotgun_kills: u64,
    pub smg_kills: u64,
    pub pistol_kills: u64,
    pub rifle_kills: u64,
    pub player_kills: u64,
    pub xp_total: u64,
    pub pve_xp: u64,
    pub pvp_xp: u64,
    pub clan_xp: u64,
    pub sharpshooter_kills: u64,
    pub survivalist_kills: u64,
    pub demolitionist_kills: u64,
    pub e_credit: u64,
    pub commendation_count: u64,
    pub commendation_score: u64,
    pub gear_score: u64,
    pub dz_rank: u64,
    pub dz_playtime: u64,
    pub rogues_killed: u64,
    pub rogue_playtime: u64,
    pub longest_rogue: u64,
    pub conflict_rank: u64,
    pub conflict_playtime: u64,
    pub all_names: Vec<String>,
}

impl Display for D2PlayerStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "玩家名：{}\n游戏时间: {}\n玩家等级: {}\nPVP击杀: {}\nNPC击杀: {}\n爆头数: {}\n爆头击杀: {}\n霰弹枪击杀: {}\n冲锋枪击杀: {}\n手枪击杀: {}\n步枪击杀: {}\n玩家击杀: {}\n总经验: {}\nPVE经验: {}\nPVP经验: {}\n战队经验: {}\n狙击专精击杀: {}\n生存专精击杀: {}\n爆破专精击杀: {}\nE币: {}\n功勋数: {}\n功勋分数: {}\n装等分数: {}\n暗区等级: {}\n暗区时间: {}\n红名击杀: {}\n红名时间: {}\n最长红名: {}\n冲突等级: {}\n冲突时间: {}\n所有名字：{:?}\n",
            self.name, self.total_playtime, self.level, self.pvp_kills, self.npc_kills, self.headshots, self.headshot_kills, self.shotgun_kills, self.smg_kills, self.pistol_kills, self.rifle_kills, self.player_kills, self.xp_total, self.pve_xp, self.pvp_xp, self.clan_xp, self.sharpshooter_kills, self.survivalist_kills, self.demolitionist_kills, self.e_credit, self.commendation_count, self.commendation_score, self.gear_score, self.dz_rank, self.dz_playtime, self.rogues_killed, self.rogue_playtime, self.longest_rogue, self.conflict_rank, self.conflict_playtime, self.all_names)
    }
}



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProfileDTO {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StatsDTO {
    pub profile: ProfileDTO,
    pub stats: Vec<Value>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UbiUser {
    pub id: String,
    pub name: String,
    pub ts: DateTime,
}

impl UbiUser {
    pub async fn store_user_name(id: &str, name: &str) -> anyhow::Result<()> {
        let db = RB.lock().await;
        db.exec(r#"
            INSERT INTO ubi_user (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING
            "#,
            vec![rbs::to_value!(id), rbs::to_value!(name)]
        ).await?;
        Ok(())
    }
    pub async fn get_user_id_by_name(name: &str) -> anyhow::Result<Vec<String>> {
        let mut db = RB.lock().await;
        let user = UbiUser::select_by_name(db.deref_mut(), &name).await?;
        Ok(user.iter().map(|x| x.id.clone()).collect())
    }
    pub async fn get_user_names_by_id(id: &str) -> anyhow::Result<Vec<String>> {
        let mut db = RB.lock().await;
        let users = UbiUser::select_by_id(db.deref_mut(), &id).await?;
        Ok(users.iter().map(|x| x.name.clone()).collect())
    }
}

crud!(UbiUser {});
impl_select!(UbiUser{select_by_name(name: &str) => "`where name = #{name}`"});
impl_select!(UbiUser{select_by_id(id: &str) => "`where id = #{id}`"});
