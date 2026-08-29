use std::path::Path;

use kakao_loco_client::api::author::Author;
use kakao_loco_client::core::command::chat::{
    ChgLogMeta, DecUnread, Feed, FeedPush, Left, MemberChange,
};
use kakao_loco_client::core::command::open_link::SyncLinkProfile;
use kakao_loco_client::network::hops::checkin::Plan;
use kakao_loco_client::network::transport::Endpoint;
use kakao_loco_client::prelude::*;

use crate::api::{ScriptAuthor, ScriptChat, ScriptMessage};
use crate::engine::Script;
use crate::error::ScriptError;

pub const ENTRY: &str = "index.js";

pub struct ScriptHost {
    scripts: Vec<Script>,
}

impl ScriptHost {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Vec::new(),
        }
    }

    pub async fn load_dir(directory: &Path) -> Result<Self, ScriptError> {
        let entry = directory.join(ENTRY);
        let code = std::fs::read_to_string(&entry).map_err(|source| ScriptError::Unreadable {
            path: entry.display().to_string(),
            source,
        })?;

        let mut host = Self::new();
        host.scripts
            .push(Script::compile_in(ENTRY, &code, directory).await?);
        Ok(host)
    }

    pub async fn add(&mut self, name: &str, code: &str) -> Result<(), ScriptError> {
        self.scripts.push(Script::compile(name, code).await?);
        Ok(())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.scripts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty()
    }

    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.scripts.iter().map(Script::name).collect()
    }

    #[must_use]
    pub fn drivers(&self) -> Vec<rquickjs::runtime::DriveFuture> {
        self.scripts.iter().map(Script::drive).collect()
    }
}

impl Default for ScriptHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptHost {
    async fn fire<A>(&self, hook: &'static str, args: A)
    where
        A: for<'js> rquickjs::function::IntoArgs<'js> + Send + Clone + 'static,
    {
        tracing::debug!(hook, "event");
        for script in &self.scripts {
            if let Err(error) = script.call(hook, args.clone()).await {
                tracing::error!(script = %script.name(), hook, %error, "script failed");
            }
        }
    }
}

impl Handler for ScriptHost {
    async fn on_message(&mut self, message: Message<'_>) -> Result<(), ClientError> {
        self.fire("onMessage", (ScriptMessage::new(&message),))
            .await;
        Ok(())
    }

    async fn on_join(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("onJoin", (ScriptChat::new(&chat), authors(members)))
            .await;
        Ok(())
    }

    async fn on_leave(&mut self, chat: Chat, members: Vec<Author>) -> Result<(), ClientError> {
        self.fire("onLeave", (ScriptChat::new(&chat), authors(members)))
            .await;
        Ok(())
    }

    async fn on_read(&mut self, chat: Chat, read: &DecUnread) -> Result<(), ClientError> {
        self.fire(
            "onRead",
            (ScriptChat::new(&chat), read.user_id, read.watermark),
        )
        .await;
        Ok(())
    }

    async fn on_log_meta(&mut self, chat: Chat, changed: &ChgLogMeta) -> Result<(), ClientError> {
        self.fire(
            "onReaction",
            (
                ScriptChat::new(&chat),
                changed.log_id,
                changed.meta_type,
                changed.content.clone(),
            ),
        )
        .await;
        Ok(())
    }

    async fn on_member_change(
        &mut self,
        chat: Chat,
        change: &MemberChange,
        members: Vec<Author>,
    ) -> Result<(), ClientError> {
        self.fire(
            "onMemberChange",
            (ScriptChat::new(&chat), change.joined, authors(members)),
        )
        .await;
        Ok(())
    }

    async fn on_feed(&mut self, chat: Chat, feed: &Feed) -> Result<(), ClientError> {
        self.fire("onFeed", (ScriptChat::new(&chat), feed.feed_type.value()))
            .await;
        Ok(())
    }

    async fn on_sync_join(&mut self, chat: Chat, _push: &FeedPush) -> Result<(), ClientError> {
        self.fire("onSyncJoin", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_link_profile(
        &mut self,
        chat: Chat,
        changed: &SyncLinkProfile,
    ) -> Result<(), ClientError> {
        self.fire("onLinkProfile", (ScriptChat::new(&chat), changed.link_id))
            .await;
        Ok(())
    }

    async fn on_left(&mut self, chat: Chat, _left: &Left) -> Result<(), ClientError> {
        self.fire("onLeft", (ScriptChat::new(&chat),)).await;
        Ok(())
    }

    async fn on_event(
        &mut self,
        _session: &Session,
        event: &SessionEvent,
    ) -> Result<(), ClientError> {
        match event {
            SessionEvent::LoggedIn { response, .. } => {
                self.fire("onLogin", (response.user_id.unwrap_or_default(),))
                    .await;
            }
            SessionEvent::Listening { ping_interval } => {
                let seconds = i64::try_from(ping_interval.as_secs()).unwrap_or_default();
                self.fire("onListening", (seconds,)).await;
            }
            SessionEvent::Push { packet, kind, .. } => {
                let method = packet.header.method.to_string();
                match kind {
                    PushKind::KickedOut(_) => self.fire("onKicked", ()).await,
                    PushKind::ChangeServer | PushKind::Restarted(_) => {
                        self.fire("onMoved", (method,)).await;
                    }
                    PushKind::MetaChanged(changed) => {
                        crate::api::chat::forget_title(changed.chat_id);
                        let meta = changed.meta.as_ref();
                        self.fire(
                            "onMetaChange",
                            (
                                changed.chat_id,
                                meta.map_or(0, |meta| meta.meta_type),
                                meta.map(|meta| meta.content.clone()).unwrap_or_default(),
                            ),
                        )
                        .await;
                    }
                    _ => self.fire("onPush", (method,)).await,
                }
            }
            SessionEvent::PingAcknowledged => {}
        }
        Ok(())
    }

    async fn on_connect(&mut self, _plan: &Plan, _endpoint: &Endpoint) -> Result<(), ClientError> {
        self.fire("onConnect", ()).await;
        Ok(())
    }

    async fn on_close(&mut self, outcome: &Result<(), ClientError>) {
        let reason = match outcome {
            Ok(()) => String::new(),
            Err(error) => error.to_string(),
        };
        self.fire("onClose", (reason,)).await;
    }
}

fn authors(members: Vec<Author>) -> Vec<ScriptAuthor> {
    members.into_iter().map(ScriptAuthor::new).collect()
}

#[cfg(test)]
mod tests {
    use super::ScriptHost;

    #[test]
    fn a_host_starts_with_nothing_loaded() {
        assert!(ScriptHost::new().is_empty());
    }

    #[tokio::test]
    async fn a_loaded_script_is_listed_by_name() {
        let mut host = ScriptHost::new();

        host.add("index.js", "globalThis.onMessage = async () => {};")
            .await
            .unwrap();

        assert_eq!(host.len(), 1);
        assert_eq!(host.names(), vec!["index.js"]);
    }

    #[tokio::test]
    async fn the_shipped_scripts_compile() {
        let host = ScriptHost::load_dir(std::path::Path::new("scripts"))
            .await
            .unwrap();

        assert_eq!(host.len(), 1, "one entry, whatever it imports");
        assert_eq!(host.names(), vec!["index.js"]);
    }

    #[tokio::test]
    async fn a_directory_without_an_entry_says_which_file_it_wanted() {
        let refused = ScriptHost::load_dir(std::path::Path::new("src")).await;

        let Err(error) = refused else {
            panic!("a directory with no index.js loaded");
        };
        assert!(error.to_string().contains("index.js"), "{error}");
    }

    #[tokio::test]
    async fn a_broken_script_names_itself_in_the_complaint() {
        let mut host = ScriptHost::new();

        let refused = host
            .add("bad.js", "globalThis.onMessage = async ( => {};")
            .await;

        let Err(error) = refused else {
            panic!("a broken script loaded");
        };
        assert!(error.is_compile(), "{error}");
        assert!(error.to_string().starts_with("bad.js"));
        assert!(host.is_empty());
    }
}

#[cfg(test)]
mod scripts {
    use std::path::Path;

    use crate::engine::Script;

    async fn probing(entry: &str) -> Script {
        Script::compile_in("probe.js", entry, Path::new("scripts"))
            .await
            .expect("진입점이 컴파일되지 않았다")
    }

    #[tokio::test]
    async fn a_prototype_name_is_not_an_item() {
        let script = probing(
            r#"
            import { lookup } from "./lib/core/registry.js";
            import "./lib/content/items.js";
            globalThis.found = String(lookup("item", "constructor"));
            globalThis.real = String(lookup("item", "강철검")?.id);
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.found").await, "null");
        assert_eq!(script.probe("globalThis.real").await, "강철검");
    }

    #[tokio::test]
    async fn a_skill_carries_its_own_rule() {
        let script = probing(
            r#"
            import { get } from "./lib/core/registry.js";
            import "./lib/content/skills.js";

            const foe = { hp: 100, maxHp: 100 };
            const me = { hp: 40, maxHp: 100 };
            globalThis.line = get("skill", "연격").act({
                me,
                foe,
                power: 8,
                strike: (target, amount) => { target.hp -= amount; return amount; },
                heal: () => 0,
                apply: () => true,
            });
            globalThis.left = String(foe.hp);
        "#,
        )
        .await;

        let line = script.probe("globalThis.line").await;
        assert!(line.contains('+'), "두 번 때리지 않았다: {line}");
        assert_ne!(script.probe("globalThis.left").await, "100");
    }

    #[tokio::test]
    async fn a_quest_counts_kills_it_never_saw() {
        let script = probing(
            r#"
            import { emit } from "./lib/core/bus.js";
            import * as registry from "./lib/core/registry.js";
            import * as quest from "./lib/game/quest.js";
            import "./lib/content/quests.js";
            import "./lib/content/monsters.js";

            globalThis.onMessage = async () => {
                const def = registry.get("quest", "슬라임 청소");
                const sheet = { id: "t", level: 5, quests: {}, bag: {} };
                quest.accept(sheet, def);

                const slime = registry.get("monster", "슬라임");
                globalThis.before = String(quest.progress(sheet, def).done);
                for (let at = 0; at < 3; at += 1) await emit("kill", { sheet, monster: slime });
                globalThis.after = String(quest.progress(sheet, def).done);
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(script.probe("globalThis.before").await, "false");
        assert_eq!(script.probe("globalThis.after").await, "true");
    }

    #[tokio::test]
    async fn the_next_edit_waits_a_second_after_the_last_one_landed() {
        let script = probing(
            r#"
            import { paced } from "./lib/core/pace.js";

            globalThis.onMessage = async () => {
                const mark = {};
                await Promise.all([
                    paced(async () => {
                        await sleep(300);        // 느린 편집 한 번
                        mark.landed = Date.now();
                    }),
                    paced(() => { mark.began = Date.now(); }),
                ]);
                globalThis.gap = mark.began - mark.landed;
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        let gap: i64 = script.probe("globalThis.gap").await.parse().unwrap_or(0);
        assert!(
            gap >= 1000,
            "앞 편집이 끝나고 {gap}ms 만에 다음 편집이 나갔다"
        );
    }

    #[tokio::test]
    async fn an_edit_does_not_chase_the_message_it_just_sent() {
        let script = probing(
            r#"
            import { mark, paced } from "./lib/core/pace.js";

            globalThis.onMessage = async () => {
                mark();                       // 방금 판을 띄웠다고 치고
                const began = Date.now();
                await paced(() => { globalThis.gap = Date.now() - began; });
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        let gap: i64 = script.probe("globalThis.gap").await.parse().unwrap_or(0);
        assert!(gap >= 1000, "메시지를 낸 지 {gap}ms 만에 편집이 나갔다");
    }

    #[tokio::test]
    async fn a_region_gate_holds() {
        let script = probing(
            r#"
            import * as world from "./lib/game/world.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";

            globalThis.onMessage = async () => {
                const sheet = blank("1", "손님", "전사");
                sheet.where = "마을";

                globalThis.shut = String((await world.travel(sheet, "용의둥지")).ok);
                globalThis.open = String((await world.travel(sheet, "어스름숲")).ok);
                globalThis.now = sheet.where;
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.shut").await,
            "false",
            "레벨이 안 되는데 들어갔다"
        );
        assert_eq!(script.probe("globalThis.open").await, "true");
        assert_eq!(script.probe("globalThis.now").await, "어스름숲");
    }

    #[tokio::test]
    async fn any_words_resolve_to_a_check() {
        let script = probing(
            r#"
            import { attempt, read } from "./lib/game/action.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/intents.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/effects.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";

            globalThis.picked = [
                "몰래 숨는다",
                "촌장을 설득한다",
                "약초를 캔다",
                "바위를 밀어 치운다",
                "갱도를 뒤져본다",
                "장터에서 노래를 부른다",
                "겁을 준다",
                "룰루랄라",
            ]
                .map((said) => read(said).id)
                .join(",");

            const sheet = blank("t", "무명", "전사");
            globalThis.every = String(
                ["...", "노래를 부른다", "바위를 든다"].every((said) => attempt(sheet, said) !== null),
            );
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.picked").await,
            "잠입,설득,채집,힘쓰기,조사,재주,위협,운",
        );
        assert_eq!(script.probe("globalThis.every").await, "true");
    }

    #[tokio::test]
    async fn a_half_typed_name_finds_one_thing_but_never_guesses() {
        let script = probing(
            r#"
            import { choose } from "./lib/core/registry.js";
            import "./lib/content/items.js";

            const near = choose("item", "젤리");
            const wide = choose("item", "검");
            globalThis.near = String(near.found?.id);
            globalThis.wide = String(wide.found);
            globalThis.many = String(wide.many.length > 1);
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.near").await, "슬라임젤리");
        assert_eq!(
            script.probe("globalThis.wide").await,
            "null",
            "애매한데도 하나를 골랐다"
        );
        assert_eq!(script.probe("globalThis.many").await, "true");
    }

    #[tokio::test]
    async fn a_person_and_a_monster_stand_the_same_way() {
        let script = probing(
            r#"
            import { fighter } from "./lib/game/battle.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/monsters.js";
            import "./lib/content/npcs.js";
            import "./lib/content/skills.js";

            const beast = fighter(registry.get("monster", "드래곤"));
            const person = fighter(registry.get("npc", "무명").fight);
            const shape = (one) => Object.keys(one).sort().join(",");

            globalThis.same = String(shape(beast) === shape(person));
            globalThis.armed = [beast.skills.length > 0, person.skills.length > 0].join(",");
            globalThis.mana = String(person.maxMp > 0);
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.same").await,
            "true",
            "둘의 꼴이 다르다"
        );
        assert_eq!(script.probe("globalThis.armed").await, "true,true");
        assert_eq!(script.probe("globalThis.mana").await, "true");
    }

    #[tokio::test]
    async fn beating_a_person_costs_you_with_them() {
        let script = probing(
            r#"
            import { emit } from "./lib/core/bus.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/game/duel.js";
            import "./lib/content/npcs.js";
            import "./lib/content/items.js";

            globalThis.onMessage = async () => {
                const smith = registry.get("npc", "볼드");
                const swordsman = registry.get("npc", "무명");
                const sheet = { id: "1", bag: {}, bonds: { 볼드: 9, 무명: 9 } };
                const said = [];
                const add = (line) => said.push(line);

                await emit("battle:end", { sheet, foe: smith.fight, won: true, add });
                await emit("battle:end", { sheet, foe: swordsman.fight, won: true, add });

                globalThis.smith = String(sheet.bonds.볼드);
                globalThis.swordsman = String(sheet.bonds.무명);
                globalThis.prize = String(sheet.bag.은장도 ?? 0);
                globalThis.said = String(said.length);
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.smith").await,
            "5",
            "이겼는데 대장장이가 그대로다"
        );
        assert_eq!(script.probe("globalThis.swordsman").await, "12");
        assert_eq!(script.probe("globalThis.prize").await, "1");
        assert_eq!(script.probe("globalThis.said").await, "3");
    }

    #[tokio::test]
    async fn recovery_does_not_leak_between_short_visits() {
        let script = probing(
            r#"
            import { breathe } from "./lib/game/time.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/phases.js";

            const often = blank("a", "자주", "전사");
            often.hp = 1;
            let dripped = 0;
            for (let at = 0; at < 20; at += 1) {
                often.at = Date.now() - 6000;
                dripped += breathe(often).hp;
            }

            const once = blank("b", "한번", "전사");
            once.hp = 1;
            once.at = Date.now() - 120000;
            const bulk = breathe(once).hp;

            globalThis.same = `${dripped},${bulk}`;
        "#,
        )
        .await;

        let both = script.probe("globalThis.same").await;
        let (often, once) = both.split_once(',').unwrap_or(("", ""));
        assert_eq!(often, once, "자주 들른 쪽이 손해를 봤다: {both}");
        assert_ne!(often, "0", "둘 다 회복하지 못했다");
    }

    #[tokio::test]
    async fn a_cooldown_counts_down_and_lets_go() {
        let script = probing(
            r#"
            import { chill, cooling } from "./lib/core/clock.js";

            const sheet = { cool: {} };
            globalThis.fresh = String(cooling(sheet, "휴식", 600));
            chill(sheet, "휴식");
            globalThis.hot = String(cooling(sheet, "휴식", 600) > 599000);

            sheet.cool.휴식 = Date.now() - 601000;
            globalThis.cold = String(cooling(sheet, "휴식", 600));
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.fresh").await, "0");
        assert_eq!(script.probe("globalThis.hot").await, "true");
        assert_eq!(script.probe("globalThis.cold").await, "0");
    }

    #[tokio::test]
    async fn what_only_comes_out_at_night_stays_in_by_day() {
        let script = probing(
            r#"
            import { foesIn } from "./lib/game/world.js";
            import "./lib/content/monsters.js";
            import "./lib/content/regions.js";
            import "./lib/content/phases.js";

            const named = (when) => foesIn("어스름숲", 99, when).map((one) => one.id);
            globalThis.day = String(named("낮").includes("그림자"));
            globalThis.night = String(named("밤").includes("그림자"));
            globalThis.map = String(named(undefined).includes("그림자"));
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.day").await, "false");
        assert_eq!(script.probe("globalThis.night").await, "true");
        assert_eq!(
            script.probe("globalThis.map").await,
            "true",
            "지도에서까지 숨었다"
        );
    }

    #[tokio::test]
    async fn a_shout_becomes_an_order_but_small_talk_does_not() {
        let script = probing(
            r#"
            import { read } from "./lib/game/order.js";
            import { blank, carry } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/effects.js";
            import "./lib/content/items.js";
            import "./lib/content/orders.js";
            import "./lib/content/regions.js";

            const sheet = blank("1", "술사", "마법사");
            carry(sheet, "고급물약", 1);

            const kind = (word, exact) => read(sheet, word, exact)?.kind ?? "없음";
            globalThis.loose = ["방어", "화염구", "고급물약", "가드", "연격", "안녕하세요"]
                .map((word) => kind(word, false))
                .join(",");

            globalThis.tight = ["방어", "물", "고급", "화염구"]
                .map((word) => kind(word, true))
                .join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.loose").await,
            "order,skill,potion,order,없음,없음",
            "익히지 않은 스킬이나 보통 말이 지시로 새어 들어왔다",
        );
        assert_eq!(
            script.probe("globalThis.tight").await,
            "order,없음,없음,skill"
        );
    }

    #[tokio::test]
    async fn everything_that_hurts_is_marked_as_such() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/effects.js";
            import "./lib/content/orders.js";

            const hurts = (one) =>
                (one.bleed ?? 0) > 0 || one.skips === true || (one.taken ?? 1) > 1 || (one.dodge ?? 0) < 0;

            globalThis.missed = registry
                .where("effect", (one) => !one.bad && hurts(one))
                .map((one) => one.id)
                .join(",") || "없음";

            globalThis.wrong = registry
                .where("effect", (one) => typeof one.onTurn !== "function" && one.bad && !hurts(one))
                .map((one) => one.id)
                .join(",") || "없음";

            globalThis.kept = registry
                .where("effect", (one) => !one.bad)
                .map((one) => one.id)
                .sort()
                .join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.missed").await,
            "없음",
            "아프게 하는데 표가 없다"
        );
        assert_eq!(
            script.probe("globalThis.wrong").await,
            "없음",
            "이롭기만 한데 표가 붙었다"
        );
        assert_eq!(
            script.probe("globalThis.kept").await,
            "방어태세,재생,축복,회피",
            "해독제가 씻어내면 안 되는 것들",
        );
    }

    #[tokio::test]
    async fn every_outcome_actually_does_something() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/intents.js";

            const TIERS = ["big", "win", "lose", "fail"];
            const gives = (step) =>
                ["exp", "gold", "item", "find", "bond", "hurt", "boon", "trouble"].some((key) => step[key]);

            globalThis.missing = registry
                .all("intent")
                .flatMap((one) => TIERS.filter((at) => !one[at]).map((at) => `${one.id}.${at}`))
                .join(",") || "없음";

            globalThis.silent = registry
                .all("intent")
                .flatMap((one) => TIERS.filter((at) => one[at] && !one[at].line).map((at) => `${one.id}.${at}`))
                .join(",") || "없음";

            globalThis.empty = registry
                .all("intent")
                .filter((one) => !gives(one.big ?? {}) || !gives(one.win ?? {}))
                .map((one) => one.id)
                .join(",") || "없음";
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.missing").await,
            "없음",
            "갈래가 빠졌다"
        );
        assert_eq!(
            script.probe("globalThis.silent").await,
            "없음",
            "할 말이 없는 갈래가 있다"
        );
        assert_eq!(
            script.probe("globalThis.empty").await,
            "없음",
            "성공해도 손에 남는 게 없다"
        );
    }

    #[tokio::test]
    async fn a_lone_boss_is_born_and_comes_back_on_a_timer() {
        let script = probing(
            r#"
            import { paceFor, regrown } from "./lib/game/nature.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/monsters.js";

            const dragon = registry.get("monster", "드래곤");
            globalThis.born = String(regrown(null, dragon).left >= 1);

            /// 먹이를 씨 말려도 보스는 제 속도
            const world = { 무리: { "용의둥지:화염도마뱀": { left: 0, at: Date.now() } } };
            globalThis.fed = String(paceFor(world, "용의둥지", dragon));

            /// 잡힌 뒤 regrow 분이 지나면 돌아온다
            const slain = { left: 0, at: Date.now() - dragon.regrow * 60000 };
            globalThis.back = String(regrown(slain, dragon, 1).left >= 1);

            /// 보스가 아닌 큰 무리는 여전히 6할에서 시작한다
            globalThis.herd = String(regrown(null, { pack: 10, regrow: 3 }).left);
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.born").await,
            "true",
            "보스가 태어나지 못한다"
        );
        assert_eq!(
            script.probe("globalThis.fed").await,
            "1",
            "보스가 먹이 때문에 굶는다"
        );
        assert_eq!(
            script.probe("globalThis.back").await,
            "true",
            "시간이 지나도 안 돌아온다"
        );
        assert_eq!(script.probe("globalThis.herd").await, "6");
    }

    #[tokio::test]
    async fn a_thinned_pack_comes_back_with_time() {
        let script = probing(
            r#"
            import { regrown } from "./lib/game/nature.js";

            const slime = { id: "슬라임", pack: 12, regrow: 3 };
            const ago = (minutes) => ({ left: 0, at: Date.now() - minutes * 60000 });

            globalThis.fresh = String(Math.round(regrown(null, slime).left * 10) / 10);
            globalThis.back = String(Math.floor(regrown(ago(30), slime).left));
            globalThis.capped = String(regrown(ago(60 * 24), slime).left);
            globalThis.boss = String(Math.floor(regrown(ago(60), { pack: 1, regrow: 180 }).left));
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.fresh").await,
            "7.2",
            "손대지 않은 세계의 시작값"
        );
        assert_eq!(script.probe("globalThis.back").await, "10", "30분에 10마리");
        assert_eq!(
            script.probe("globalThis.capped").await,
            "12",
            "무리 크기를 넘어 불어났다"
        );
        assert_eq!(
            script.probe("globalThis.boss").await,
            "0",
            "한 시간 만에 보스가 돌아왔다"
        );
    }

    #[tokio::test]
    async fn only_the_quick_can_skip_a_region() {
        let script = probing(
            r#"
            import { pathTo, reach } from "./lib/game/world.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";

            const slow = blank("a", "둔한", "전사");
            const quick = blank("b", "빠른", "도적");
            quick.level = 9;

            globalThis.reaches = [reach(slow), reach(quick)].join(",");
            globalThis.blocked = String(pathTo(slow, "버려진광산"));
            globalThis.hops = String(pathTo(quick, "안개늪")?.length);
            globalThis.route = (pathTo(quick, "안개늪") ?? []).join(">");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.reaches").await,
            "1,3",
            "느린 사람과 빠른 사람의 걸음이 같다"
        );
        assert_eq!(
            script.probe("globalThis.blocked").await,
            "null",
            "느린 사람이 두 구역을 건넜다"
        );
        assert_eq!(script.probe("globalThis.hops").await, "3");
        assert_eq!(
            script.probe("globalThis.route").await,
            "어스름숲>버려진광산>안개늪"
        );
    }

    #[tokio::test]
    async fn an_old_sheet_is_fitted_to_the_rules_it_never_knew() {
        let script = probing(
            r#"
            import { normalize, stats } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";

            const old = {
                id: "1", name: "테스트", class: "전사", level: 1, exp: 8,
                hp: 24, maxHp: 26, power: 4, gold: 42, wins: 1, deaths: 0,
                weapon: "낡은검", armor: "천옷", bag: { 물약: 2 }, quest: null,
            };
            const now = normalize(old);

            globalThis.shape = [
                Array.isArray(now.skills) && now.skills.length > 0,
                now.gear.weapon === "낡은검",
                now.where === "마을",
                Number.isFinite(now.mp),
                now.maxHp === undefined,
                now.gold === 42,
            ].join(",");
            globalThis.armed = String(stats(now).power > 0);
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.shape").await,
            "true,true,true,true,true,true",
        );
        assert_eq!(script.probe("globalThis.armed").await, "true");
    }

    #[tokio::test]
    async fn a_finished_step_stays_finished() {
        let script = probing(
            r#"
            import { current } from "./lib/game/guide.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/steps.js";

            const sheet = blank("1", "손님", "전사");
            sheet.where = "어스름숲";
            current(sheet);

            sheet.where = "마을";
            current(sheet);
            globalThis.still = String((sheet.guide?.cleared ?? []).includes("바깥"));
            globalThis.next = String(current(sheet)?.id);
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.still").await, "true");
        assert_eq!(script.probe("globalThis.next").await, "몸풀기");
    }

    #[tokio::test]
    async fn what_eats_what_moves_the_numbers() {
        let script = probing(
            r#"
            import { paceFor } from "./lib/game/nature.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/monsters.js";

            const world = { 무리: {} };
            const set = (id, left) => {
                world.무리[`어스름숲:${id}`] = { left, at: Date.now() };
            };
            const slime = registry.get("monster", "슬라임");
            const goblin = registry.get("monster", "고블린");
            const pace = (def) => paceFor(world, "어스름숲", def);

            const hunters = registry.where(
                "monster",
                (one) => (one.eats ?? []).includes("슬라임") && (one.where ?? []).includes("어스름숲"),
            );
            globalThis.web = String(hunters.length >= 2);

            set("슬라임", 12);
            for (const one of hunters) set(one.id, one.pack ?? 6);
            globalThis.pressed = String(pace(slime) < 0.5);

            for (const one of hunters) set(one.id, 0);
            globalThis.free = String(pace(slime) >= 1);

            set("슬라임", 0);
            set("고블린", 5);
            globalThis.starving = String(pace(goblin) < 0);

            set("슬라임", 12);
            globalThis.fed = String(pace(goblin) > 0);
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.web").await,
            "true",
            "먹이를 노리는 것이 하나뿐이다"
        );
        assert_eq!(
            script.probe("globalThis.pressed").await,
            "true",
            "포식자가 가득인데 먹이가 제 속도로 는다"
        );
        assert_eq!(
            script.probe("globalThis.free").await,
            "true",
            "포식자가 없는데도 눌려 있다"
        );
        assert_eq!(
            script.probe("globalThis.starving").await,
            "true",
            "먹이가 없는데 굶지 않는다"
        );
        assert_eq!(script.probe("globalThis.fed").await, "true");
    }

    #[tokio::test]
    async fn a_quest_can_wait_for_the_world_to_change() {
        let script = probing(
            r#"
            import { meets } from "./lib/game/quest.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/quests.js";
            import "./lib/content/monsters.js";

            const thick = { monster: "오크", thick: 0.7 };
            const thin = { monster: "슬라임", thin: 0.3 };

            globalThis.gated = [
                meets(thick, [0.6, 0.6]),
                meets(thick, [0.6, 0.9]),
                meets(thin, [0.25]),
                meets(thin, [0.5]),
                meets(undefined, []),
            ].join(",");

            globalThis.broken = registry
                .where("quest", (one) => one.needs)
                .filter(
                    (one) =>
                        !registry.get("monster", one.needs.monster) ||
                        (one.needs.thick === undefined && one.needs.thin === undefined),
                )
                .map((one) => one.id)
                .join(",") || "없음";

            globalThis.both = registry.where("quest", (one) => one.needs).length;
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.gated").await,
            "false,true,true,false,true"
        );
        assert_eq!(
            script.probe("globalThis.broken").await,
            "없음",
            "없는 몬스터를 보는 의뢰가 있다"
        );
        assert_ne!(script.probe("globalThis.both").await, "0");
    }

    #[tokio::test]
    async fn a_person_points_at_a_command_that_exists() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/npcs.js";
            import "./lib/commands/character.js";
            import "./lib/commands/adventure.js";
            import "./lib/commands/social.js";
            import "./lib/commands/meta.js";

            globalThis.dangling = registry
                .where("npc", (one) => one.command)
                .filter((one) => !registry.get("command", one.command))
                .map((one) => `${one.id}→${one.command}`)
                .join(",") || "없음";

            globalThis.bare = registry
                .where("npc", (one) => one.service && !one.command && !one.about)
                .map((one) => one.id)
                .join(",") || "없음";

            const inns = registry.where("npc", (one) => one.command === "여관");
            globalThis.inn = [inns.some((one) => one.id === "순덕"), inns.length >= 3].join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.dangling").await,
            "없음",
            "없는 명령을 가리킨다"
        );
        assert_eq!(
            script.probe("globalThis.bare").await,
            "없음",
            "설명 없는 딱지가 붙어 있다"
        );
        assert_eq!(
            script.probe("globalThis.inn").await,
            "true,true",
            "묵을 곳이 모자라다"
        );
    }

    #[tokio::test]
    async fn the_world_holds_together() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/classes.js";
            import "./lib/content/jobs.js";
            import "./lib/content/skills.js";
            import "./lib/content/effects.js";
            import "./lib/content/items.js";
            import "./lib/content/monsters.js";
            import "./lib/content/regions.js";
            import "./lib/content/npcs.js";
            import "./lib/content/quests.js";
            import "./lib/content/orders.js";
            import "./lib/content/areas.js";
            import "./lib/content/recipes.js";
            import "./lib/content/markets.js";

            const said = (list) => list.join(",") || "없음";

            /// 장은 마을에만 서고, 있는 물건만 다룬다.
            globalThis.trade = said(
                registry.all("market").flatMap((one) => [
                    ...(registry.get("region", one.id)?.safe ? [] : [`${one.id} 장?`]),
                    ...[...(one.local ?? []), ...(one.wants ?? [])].filter((id) => !registry.get("item", id)).map((id) => `${one.id}:${id}?`),
                ]),
            );

            globalThis.cooking = said(
                registry.all("recipe").flatMap((recipe) => [
                    ...(registry.get("item", recipe.id) ? [] : [`${recipe.id}?`]),
                    ...Object.keys(recipe.takes ?? {}).filter((id) => !registry.get("item", id)).map((id) => `${recipe.id}←${id}?`),
                    ...(recipe.at ?? []).filter((id) => !registry.get("npc", id)).map((id) => `${recipe.id}@${id}?`),
                    ...(Object.keys(recipe.takes ?? {}).includes(recipe.id) ? [`${recipe.id} 자기 자신`] : []),
                ]),
            );
            const regions = registry.all("region");

            globalThis.lands = said([
                ...regions.filter((one) => !registry.get("area", one.area)).map((one) => `${one.id}@${one.area}`),
                ...registry.all("area").filter((area) => !regions.some((one) => one.area === area.id)).map((area) => `빈 ${area.id}`),
            ]);

            globalThis.roads = said(
                regions.flatMap((one) =>
                    one.links
                        .filter((to) => !registry.get("region", to) || !registry.get("region", to).links.includes(one.id))
                        .map((to) => `${one.id}→${to}`),
                ),
            );

            const seen = new Set(["마을"]);
            for (let step = 0; step < regions.length; step += 1) {
                for (const one of regions) {
                    if (!seen.has(one.id)) continue;
                    for (const to of one.links) seen.add(to);
                }
            }
            globalThis.cutoff = said(regions.filter((one) => !seen.has(one.id)).map((one) => one.id));

            /// 사냥터인데 아무것도 안 사는 곳
            globalThis.barren = said(
                regions
                    .filter((one) => !one.safe)
                    .filter((one) => !registry.all("monster").some((m) => (m.where ?? []).includes(one.id)))
                    .map((one) => one.id),
            );

            /// 없는 물건을 떨구거나 팔거나 주는 것
            const items = (list) => (list ?? []).filter((id) => !registry.get("item", id));
            globalThis.ghosts = said([
                ...registry.all("monster").flatMap((one) => items((one.loot ?? []).map((d) => d.id)).map((id) => `${one.id}:${id}`)),
                ...regions.flatMap((one) => items((one.finds ?? []).flatMap((f) => f.pool ?? [])).map((id) => `${one.id}:${id}`)),
                ...registry.all("npc").flatMap((one) => items(one.sells).map((id) => `${one.id}:${id}`)),
                ...registry.all("quest").flatMap((one) => items(one.prize ? [one.prize] : []).map((id) => `${one.id}:${id}`)),
            ]);

            /// 없는 사람이 주는 의뢰, 없는 곳에 사는 사람
            globalThis.orphans = said([
                ...registry.all("quest").filter((one) => !registry.get("npc", one.giver)).map((one) => `${one.id}←${one.giver}`),
                ...registry.all("npc").filter((one) => one.where !== "떠돎" && !registry.get("region", one.where)).map((one) => `${one.id}@${one.where}`),
            ]);

            globalThis.guides = said(
                [...new Set(registry.all("class").map((one) => one.tier).filter((tier) => tier >= 2))]
                    .filter((tier) => !registry.all("npc").some((one) => one.advances === tier))
                    .map((tier) => `${tier}차 안내자 없음`),
            );

            globalThis.roads2 = said(
                registry.all("class").flatMap((one) => [
                    ...(one.next ?? []).filter((id) => !registry.get("class", id)).map((id) => `${one.id}→${id}?`),
                    ...(one.next ?? []).filter((id) => registry.get("class", id) && registry.get("class", id).from !== one.id).map((id) => `${one.id}→${id} 출신 불일치`),
                    ...(one.from && !registry.get("class", one.from) ? [`${one.id}←${one.from}?`] : []),
                ]),
            );

            /// 없는 재주를 쓰는 것 — 트리도 본다
            const skills = (list) => (list ?? []).filter((id) => !registry.get("skill", id));
            globalThis.unknown = said([
                ...registry.all("class").flatMap((one) => skills(one.skills).map((id) => `${one.id}:${id}`)),
                ...registry.all("class").flatMap((one) => skills(Object.values(one.tree ?? {}).flat()).map((id) => `${one.id}:${id}`)),
                ...registry.all("monster").flatMap((one) => skills(one.skills).map((id) => `${one.id}:${id}`)),
                ...registry.all("npc").flatMap((one) => skills(one.fight?.skills).map((id) => `${one.id}:${id}`)),
            ]);

            globalThis.size = [
                regions.length >= 15,
                registry.all("item").length >= 60,
                registry.all("monster").length >= 25,
                registry.where("region", (one) => one.safe).length >= 5,
            ].join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.trade").await,
            "없음",
            "장이 없는 물건이나 마을 아닌 곳을 가리킨다"
        );
        assert_eq!(
            script.probe("globalThis.cooking").await,
            "없음",
            "레시피가 없는 것을 가리킨다"
        );
        assert_eq!(
            script.probe("globalThis.lands").await,
            "없음",
            "땅이 없는 지역이나 빈 땅이 있다"
        );
        assert_eq!(
            script.probe("globalThis.roads").await,
            "없음",
            "한쪽만 이어진 길이 있다"
        );
        assert_eq!(
            script.probe("globalThis.cutoff").await,
            "없음",
            "마을에서 못 가는 곳이 있다"
        );
        assert_eq!(
            script.probe("globalThis.barren").await,
            "없음",
            "아무것도 안 사는 사냥터가 있다"
        );
        assert_eq!(
            script.probe("globalThis.ghosts").await,
            "없음",
            "없는 물건을 가리킨다"
        );
        assert_eq!(
            script.probe("globalThis.orphans").await,
            "없음",
            "없는 사람이나 없는 곳을 가리킨다"
        );
        assert_eq!(
            script.probe("globalThis.guides").await,
            "없음",
            "전직 안내자가 없는 차수가 있다"
        );
        assert_eq!(
            script.probe("globalThis.roads2").await,
            "없음",
            "갈림길이 끊겼거나 출신이 안 맞는다"
        );
        assert_eq!(
            script.probe("globalThis.unknown").await,
            "없음",
            "없는 재주를 쓴다"
        );
        assert_eq!(
            script.probe("globalThis.size").await,
            "true,true,true,true",
            "세계가 줄었다"
        );
    }

    #[tokio::test]
    async fn an_old_level_is_recounted_on_the_new_curve() {
        let script = probing(
            r#"
            import { CEILING, CURVE, normalize, relevel } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";

            /// 옛 곡선으로 35까지 오른 사람
            const old = { id: "1", name: "민들레꽃", class: "사제", level: 35, exp: 16734, hp: 2284, gold: 1 };
            const now = normalize(old);
            globalThis.capped = String(now.level <= CEILING);
            globalThis.high = String(now.level >= 20);
            globalThis.kept = String(now.was?.level);
            globalThis.stamped = String(now.curve === CURVE);

            /// 이미 새 곡선인 시트는 건드리지 않는다
            const fresh = { level: 7, exp: 10, curve: CURVE };
            relevel(fresh);
            globalThis.untouched = [fresh.level, fresh.exp, fresh.was === undefined].join(",");

            /// 1레벨 0경험치는 어느 곡선이든 1레벨이다
            const zero = relevel({ level: 1, exp: 0 });
            globalThis.zero = [zero.level, zero.exp, zero.was === undefined].join(",");

            const { gain, need } = await import("./lib/game/sheet.js");
            gain(now, 999999999);
            globalThis.stuck = [now.level === CEILING, now.exp <= need(now)].join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.capped").await,
            "true",
            "천장을 넘었다"
        );
        assert_eq!(
            script.probe("globalThis.high").await,
            "true",
            "번 만큼 인정받지 못했다"
        );
        assert_eq!(
            script.probe("globalThis.kept").await,
            "35",
            "되돌릴 값을 안 남겼다"
        );
        assert_eq!(script.probe("globalThis.stamped").await, "true");
        assert_eq!(
            script.probe("globalThis.untouched").await,
            "7,10,true",
            "새 곡선 시트를 건드렸다"
        );
        assert_eq!(script.probe("globalThis.zero").await, "1,0,true");
        assert_eq!(
            script.probe("globalThis.stuck").await,
            "true,true",
            "천장 위로 튀었다"
        );
    }

    #[tokio::test]
    async fn two_people_can_fight_and_the_stake_moves() {
        let script = probing(
            r#"
            import { sandbox } from "./test/memfs.js";
            import * as battle from "./lib/game/battle.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/effects.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/phases.js";
            import "./lib/content/orders.js";
            import "./lib/content/traits.js";

            globalThis.onMessage = async () => {
                const { restore } = sandbox();
                try {
                const a = blank("a", "갑", "전사"); a.level = 6; a.gold = 1000;
                const b = blank("b", "을", "도적"); b.level = 6; b.gold = 1000;
                const msg = { say: async () => "log", chat: { edit: async () => {} } };

                const both = [];
                const going = battle.spar(msg, a, b, "마을");
                await new Promise((done) => setTimeout(done, 5));
                both.push(battle.inFight("a"), battle.inFight("b"));
                both.push(battle.order("b", { kind: "order", order: { id: "방어", icon: "🛡", act: ({ me, apply }) => { apply(me, "방어태세"); return "웅크린다"; } } }) === null);
                await going;

                const winner = a.wins === 1 ? a : b.wins === 1 ? b : null;
                const loser = winner === a ? b : a;
                globalThis.decided = String(winner !== null && loser.deaths === 1);
                globalThis.stake = String(winner ? winner.gold > 1000 && loser.gold < 1000 && winner.gold + loser.gold === 2000 : false);
                globalThis.written = String(loser.hp === 0 || loser.hp > 0);
                globalThis.freed = String(!battle.inFight("a") && !battle.inFight("b"));
                globalThis.both = both.join(",");
                } finally { restore(); }
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.both").await,
            "true,true,true",
            "한쪽만 판에 섰거나 지시를 못 넣는다"
        );
        assert_eq!(
            script.probe("globalThis.decided").await,
            "true",
            "승부가 안 났다"
        );
        assert_eq!(
            script.probe("globalThis.stake").await,
            "true",
            "판돈이 안 옮겨졌거나 새나갔다"
        );
        assert_eq!(
            script.probe("globalThis.freed").await,
            "true",
            "끝난 뒤에도 싸우는 중이다"
        );
    }

    #[tokio::test]
    async fn crafting_turns_stuff_into_things() {
        let script = probing(
            r#"
            import * as craft from "./lib/game/craft.js";
            import * as registry from "./lib/core/registry.js";
            import { blank, carry, count } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/npcs.js";
            import "./lib/content/recipes.js";

            globalThis.onMessage = async () => {
                const sheet = blank("1", "손", "전사");
                sheet.level = 3;
                const potion = registry.get("recipe", "물약");
                const sword = registry.get("recipe", "강철검");
                const smith = registry.get("npc", "볼드");

                /// 재료가 없으면 못 만든다
                globalThis.short = String(craft.ready(sheet, potion, []).ok);

                /// 들판에서도 약은 달인다
                carry(sheet, "약초", 2);
                globalThis.field = String(craft.ready(sheet, potion, []).ok);
                await craft.make(sheet, potion);
                globalThis.after = [count(sheet, "약초"), count(sheet, "물약")].join(",");

                /// 칼은 대장장이 앞에서만
                carry(sheet, "쇳가루", 3); carry(sheet, "은화석", 1);
                globalThis.nowhere = String(craft.ready(sheet, sword, []).ok);
                globalThis.forge = String(craft.ready(sheet, sword, [smith]).ok);

                /// 레벨이 안 되면 사람 앞이라도 안 된다
                sheet.level = 1;
                globalThis.green = String(craft.ready(sheet, sword, [smith]).ok);
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.short").await,
            "false",
            "재료 없이 만들었다"
        );
        assert_eq!(
            script.probe("globalThis.field").await,
            "true",
            "들판에서 약을 못 달인다"
        );
        assert_eq!(
            script.probe("globalThis.after").await,
            "0,3",
            "재료가 안 빠지거나 물건이 안 들어왔다"
        );
        assert_eq!(
            script.probe("globalThis.nowhere").await,
            "false",
            "대장장이 없이 칼을 쳤다"
        );
        assert_eq!(script.probe("globalThis.forge").await, "true");
        assert_eq!(
            script.probe("globalThis.green").await,
            "false",
            "레벨 문턱이 없다"
        );
    }

    #[tokio::test]
    async fn a_title_sticks_once_earned() {
        let script = probing(
            r#"
            import * as honor from "./lib/game/honor.js";
            import { emit } from "./lib/core/bus.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/titles.js";

            globalThis.onMessage = async () => {
                const sheet = blank("1", "손", "전사");
                globalThis.none = String(honor.award(sheet).length);

                sheet.wins = 1;
                const first = honor.award(sheet);
                globalThis.first = first.map((one) => one.id).join(",");
                globalThis.worn = String(sheet.title);
                globalThis.again = String(honor.award(sheet).length);

                /// 조건이 도로 거짓이 돼도 칭호는 남는다
                sheet.wins = 0;
                globalThis.kept = String(sheet.titles.includes("첫걸음"));

                /// 못 얻은 칭호는 못 단다
                globalThis.refused = String(honor.wear(sheet, "용살자"));

                /// 전투 끝에 한 줄로 얹힌다
                const said = [];
                sheet.kills = { 드래곤: 1 };
                await emit("battle:end", { sheet, foe: {}, won: true, add: (line) => said.push(line) });
                globalThis.said = said.join("|");
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(script.probe("globalThis.none").await, "0");
        assert_eq!(script.probe("globalThis.first").await, "첫걸음");
        assert_eq!(
            script.probe("globalThis.worn").await,
            "첫걸음",
            "첫 칭호가 저절로 달리지 않았다"
        );
        assert_eq!(
            script.probe("globalThis.again").await,
            "0",
            "같은 칭호가 두 번 붙었다"
        );
        assert_eq!(
            script.probe("globalThis.kept").await,
            "true",
            "칭호가 떨어졌다"
        );
        assert_eq!(script.probe("globalThis.refused").await, "false");
        assert!(
            script.probe("globalThis.said").await.contains("용살자"),
            "전투 끝에 칭호가 안 얹혔다"
        );
    }

    #[tokio::test]
    async fn a_head_price_rises_and_is_claimed() {
        let script = probing(
            r#"
            import { apply } from "./lib/game/bounty.js";

            const board = {};
            const a = { id: "a", name: "갑", level: 6, gold: 1000 };
            const b = { id: "b", name: "을", level: 6, gold: 1000 };

            apply(board, { winner: a, loser: b, stake: 100, fled: false });
            globalThis.rose = String((board.a?.amount ?? 0) > 0 && !board.b);

            const before = b.gold;
            const out = apply(board, { winner: b, loser: a, stake: 100, fled: false });
            globalThis.claimed = String(b.gold > before && !board.a && (board.b?.amount ?? 0) > 0);
            globalThis.told = String(out.lines.length);

            /// 달아난 판은 아무것도 안 바꾼다
            const still = JSON.stringify(board);
            const quiet = apply(board, { winner: a, loser: b, stake: 50, fled: true });
            globalThis.quiet = String(!quiet.changed && JSON.stringify(board) === still);
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.rose").await,
            "true",
            "이겼는데 목값이 안 붙었다"
        );
        assert_eq!(
            script.probe("globalThis.claimed").await,
            "true",
            "목값을 못 가져갔거나 안 지워졌다"
        );
        assert_eq!(
            script.probe("globalThis.told").await,
            "2",
            "가져간 것과 새로 붙은 것 두 줄이어야 한다"
        );
        assert_eq!(
            script.probe("globalThis.quiet").await,
            "true",
            "달아난 판이 목값을 바꿨다"
        );
    }

    #[tokio::test]
    async fn a_rumor_only_spreads_when_the_world_gives_it_reason() {
        let script = probing(
            r#"
            import { choose } from "./lib/game/rumor.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/rumors.js";

            const quiet = { sheet: { where: "마을" }, world: {}, phase: { id: "낮" }, packs: {}, bounties: [], duel: null, slain: null };
            globalThis.silent = String(choose(quiet));

            const loud = { ...quiet, packs: { 버려진광산: { 오크: 0.9 } } };
            globalThis.orc = String(choose(loud)?.includes("오크"));

            const gossip = { ...quiet, duel: { winner: "갑", loser: "을", where: "마을", at: Date.now() } };
            globalThis.duel = String(choose(gossip)?.includes("갑"));

            const stale = { ...quiet, duel: { winner: "갑", loser: "을", where: "마을", at: Date.now() - 7 * 3600 * 1000 } };
            globalThis.stale = String(choose(stale));

            globalThis.shape = String(registry.all("rumor").every((one) => typeof one.when === "function" && typeof one.tell === "function"));
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.silent").await,
            "null",
            "아무 일 없는데 소문이 돈다"
        );
        assert_eq!(
            script.probe("globalThis.orc").await,
            "true",
            "오크 소문이 안 돈다"
        );
        assert_eq!(
            script.probe("globalThis.duel").await,
            "true",
            "결투 소문이 안 돈다"
        );
        assert_eq!(
            script.probe("globalThis.stale").await,
            "null",
            "여섯 시간 지난 결투를 아직 떠든다"
        );
        assert_eq!(script.probe("globalThis.shape").await, "true");
    }

    #[tokio::test]
    async fn skills_open_with_level_and_hands_hold_four() {
        let script = probing(
            r#"
            import * as skill from "./lib/game/skill.js";
            import { blank, normalize } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";

            const s = blank("1", "손", "전사");
            globalThis.start = skill.known(s).length;

            s.level = 12;
            const now = skill.known(s);
            globalThis.opened = [now.includes("돌진"), now.includes("분쇄"), now.includes("전쟁함성")].join(",");
            globalThis.next = String(skill.locked(s)[0]?.level);

            /// 손은 넷
            const tries = ["돌진", "철벽", "분쇄"].map((id) => skill.equip(s, id).ok);
            globalThis.hands = [s.skills.length, tries.join("|")].join(";");
            globalThis.full = String(skill.equip(s, "도발").ok === false || s.skills.length <= skill.SLOTS);

            const down = skill.unequip(s, "돌진").ok;
            const up = skill.equip(s, "돌진").ok;
            for (const id of [...s.skills]) skill.unequip(s, id);
            globalThis.swap = [down, up, s.skills.length].join(",");

            /// 배운 건 손이 꽉 차도 남는다
            s.skills = ["방패베기", "도발", "돌진", "철벽"];
            const taught = skill.learn(s, "연격");
            globalThis.learned = [taught.ok, skill.known(s).includes("연격")].join(",");

            /// 옛 시트: 다섯을 들고 있고 그중 하나는 트리에 없다
            const old = normalize({ id: "2", name: "옛", class: "도적", level: 3, skills: ["급습", "연막", "흡혈", "역전", "독칼"], gold: 1 });
            globalThis.fitted = [old.skills.length <= skill.SLOTS, old.taught.includes("흡혈"), old.taught.includes("역전")].join(",");
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.start").await, "2");
        assert_eq!(
            script.probe("globalThis.opened").await,
            "true,true,false",
            "레벨대로 안 열린다"
        );
        assert_eq!(script.probe("globalThis.next").await, "16");
        assert_eq!(
            script.probe("globalThis.hands").await,
            "4;true|true|false",
            "손이 넷을 넘거나 못 채운다"
        );
        assert_eq!(script.probe("globalThis.full").await, "true");
        assert_eq!(
            script.probe("globalThis.swap").await,
            "true,true,1",
            "하나도 안 남기고 내려놓았다"
        );
        assert_eq!(
            script.probe("globalThis.learned").await,
            "false,true",
            "손이 꽉 찼는데 배운 게 안 남는다"
        );
        assert_eq!(
            script.probe("globalThis.fitted").await,
            "true,true,true",
            "옛 시트를 잘못 접었다"
        );
    }

    #[tokio::test]
    async fn a_raid_is_counted_together_and_breaks_the_town_when_missed() {
        let script = probing(
            r#"
            import * as raid from "./lib/game/raid.js";

            const t0 = 1_000_000;
            const goblin = { id: "고블린", icon: "👺" };
            const world = {};

            const event = raid.spawn(world, { monster: goblin, town: "마을", players: 2, at: t0 });
            globalThis.need = String(event.need);
            globalThis.capped = String(raid.spawn({}, { monster: goblin, town: "마을", players: 99, at: t0 }).need);

            raid.count(world, "a", "고블린", t0 + 1);
            raid.count(world, "b", "고블린", t0 + 2);
            raid.count(world, "a", "슬라임", t0 + 3);
            globalThis.sofar = [raid.total(world.사건), world.사건.got.a, world.사건.got.b].join(",");

            /// 채우면 막은 것이다
            for (let at = 0; at < 20; at += 1) raid.count(world, "b", "고블린", t0 + 10);
            globalThis.held = String(world.사건.done);
            globalThis.overshoot = String(raid.total(world.사건) <= world.사건.need);

            /// 시간이 지나면 뚫리고, 마을이 닫힌다
            const late = {};
            raid.spawn(late, { monster: goblin, town: "쇠터", players: 1, at: t0 });
            raid.count(late, "a", "고블린", t0 + 1);
            globalThis.early = String(raid.expire(late, t0 + 1000));
            const broke = raid.expire(late, t0 + raid.LASTS + 1);
            globalThis.broke = [broke?.done, String(!!raid.closed(late, "쇠터", t0 + raid.LASTS + 2)), String(!!raid.closed(late, "마을", t0 + raid.LASTS + 2))].join(",");
            globalThis.reopened = String(!raid.closed(late, "쇠터", t0 + raid.LASTS + raid.SHUT + 10));

            /// 끝난 사건에는 더 안 센다
            globalThis.dead = String(raid.count(late, "a", "고블린", t0 + raid.LASTS + 5));
        "#,
        )
        .await;

        assert_eq!(script.probe("globalThis.need").await, "12", "둘이면 열둘");
        assert_eq!(script.probe("globalThis.capped").await, "30", "상한이 없다");
        assert_eq!(
            script.probe("globalThis.sofar").await,
            "2,1,1",
            "다른 놈을 셌거나 나눠 세지 못했다"
        );
        assert_eq!(script.probe("globalThis.held").await, "막음");
        assert_eq!(
            script.probe("globalThis.overshoot").await,
            "true",
            "막은 뒤에도 계속 셌다"
        );
        assert_eq!(
            script.probe("globalThis.early").await,
            "null",
            "시간 전에 뚫렸다"
        );
        assert_eq!(
            script.probe("globalThis.broke").await,
            "뚫림,true,false",
            "뚫렸는데 마을이 안 닫히거나 엉뚱한 마을이 닫혔다"
        );
        assert_eq!(
            script.probe("globalThis.reopened").await,
            "true",
            "다시 열리지 않는다"
        );
        assert_eq!(script.probe("globalThis.dead").await, "null");
    }

    #[tokio::test]
    async fn prices_differ_by_town() {
        let script = probing(
            r#"
            import { rate, whereFor } from "./lib/game/market.js";
            import "./lib/content/markets.js";

            globalThis.iron = [rate("쇠터", "쇳가루", "buy"), rate("쇠터", "쇳가루", "sell"), rate("갈대나루", "쇳가루", "sell"), rate("마을", "용비늘", "sell")].join(",");
            globalThis.wild = String(rate("어스름숲", "쇳가루", "sell"));
            globalThis.spots = [whereFor("쇳가루").cheap.join("/"), whereFor("쇳가루").dear.join("/")].join(";");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.iron").await,
            "0.75,0.7,1.6,1",
            "배율이 어긋난다"
        );
        assert_eq!(
            script.probe("globalThis.wild").await,
            "1",
            "장이 없는 곳에 배율이 붙었다"
        );
        assert_eq!(
            script.probe("globalThis.spots").await,
            "쇠터;마을/갈대나루",
            "싼 곳과 비싼 곳이 틀렸다"
        );
    }

    #[tokio::test]
    async fn only_the_worn_title_does_anything() {
        let script = probing(
            r#"
            import { blank, perk, stats } from "./lib/game/sheet.js";
            import { reach } from "./lib/game/world.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";
            import "./lib/content/titles.js";

            const s = blank("1", "손", "전사");
            s.titles = ["백전", "떠돌이"];
            const bare = { guard: stats(s).guard, reach: reach(s) };

            s.title = "백전";
            const armored = { guard: stats(s).guard, reach: reach(s) };
            s.title = "떠돌이";
            const roaming = { guard: stats(s).guard, reach: reach(s) };

            globalThis.guard = [bare.guard, armored.guard, roaming.guard].join(",");
            globalThis.reach = [bare.reach, armored.reach, roaming.reach].join(",");
            globalThis.none = String(perk({ title: "없는칭호" }, "guard"));

            globalThis.mute = registry.where("title", (one) => one.perk && !one.note).map((one) => one.id).join(",") || "없음";
        "#,
        )
        .await;

        let guard = script.probe("globalThis.guard").await;
        let parts: Vec<i64> = guard.split(',').map(|p| p.parse().unwrap()).collect();
        assert_eq!(
            parts[1],
            parts[0] + 2,
            "백전을 달았는데 방어가 안 올랐다: {guard}"
        );
        assert_eq!(
            parts[2], parts[0],
            "바꿔 달았는데 옛 효과가 남았다: {guard}"
        );
        assert_eq!(
            script.probe("globalThis.reach").await,
            "1,1,2",
            "떠돌이를 달았는데 걸음이 그대로다"
        );
        assert_eq!(script.probe("globalThis.none").await, "0");
        assert_eq!(
            script.probe("globalThis.mute").await,
            "없음",
            "설명 없는 효과가 있다"
        );
    }

    #[tokio::test]
    async fn a_friend_can_join_a_fight_and_both_are_paid() {
        let script = probing(
            r#"
            import { sandbox } from "./test/memfs.js";
            import * as battle from "./lib/game/battle.js";
            import * as quest from "./lib/game/quest.js";
            import * as registry from "./lib/core/registry.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/effects.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/quests.js";
            import "./lib/content/phases.js";
            import "./lib/content/orders.js";
            import "./lib/content/traits.js";

            globalThis.onMessage = async () => {
                const { restore } = sandbox();
                try {
                const a = blank("a", "갑", "전사"); a.level = 8;
                const b = blank("b", "을", "도적"); b.level = 8;
                const chore = registry.get("quest", "슬라임 청소");
                quest.accept(a, chore); quest.accept(b, chore);

                const dummy = { ...registry.get("monster", "슬라임"), hp: 90, power: 3, traits: [] };
                const msg = { say: async () => "log", chat: { edit: async () => {} } };

                const going = battle.start(msg, a, dummy, "마을");
                await new Promise((done) => setTimeout(done, 5));
                const joined = battle.join(b, "a");
                const both = [battle.inFight("a"), battle.inFight("b")];
                const twice = battle.join(b, "a");
                await going;

                globalThis.joined = String(joined);
                globalThis.both = both.join(",");
                globalThis.twice = String(twice !== null);
                globalThis.paid = [a.wins, b.wins, a.gold > 40, b.gold > 40].join(",");
                globalThis.chore = [a.quests[chore.id].got, b.quests[chore.id].got].join(",");
                globalThis.freed = String(!battle.inFight("a") && !battle.inFight("b"));

                /// 결투에는 못 낀다
                const c = blank("c", "병", "사제"); c.level = 8;
                const d = blank("d", "정", "궁수"); d.level = 8;
                const duel = battle.spar(msg, c, d, "마을");
                await new Promise((done) => setTimeout(done, 5));
                globalThis.duel = String(battle.join(a, "c"));
                await duel;
                } finally { restore(); }
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.joined").await,
            "null",
            "끼어들지 못했다"
        );
        assert_eq!(
            script.probe("globalThis.both").await,
            "true,true",
            "둘 다 판에 서지 않았다"
        );
        assert_eq!(
            script.probe("globalThis.twice").await,
            "true",
            "두 번 끼어들었다"
        );
        assert_eq!(
            script.probe("globalThis.paid").await,
            "1,1,true,true",
            "둘 다 못 받았다"
        );
        assert_eq!(
            script.probe("globalThis.chore").await,
            "1,1",
            "의뢰가 한쪽만 올랐다"
        );
        assert_eq!(script.probe("globalThis.freed").await, "true");
        assert_eq!(
            script.probe("globalThis.duel").await,
            "결투에는 끼어들 수 없다"
        );
    }

    #[tokio::test]
    async fn the_raid_ticker_starts_on_what_the_engine_actually_ships() {
        let script = probing(
            r#"
            import { start } from "./lib/game/raid.js";
            import "./lib/content/monsters.js";
            import "./lib/content/regions.js";

            globalThis.interval = String(typeof setInterval);
            try {
                start();
                start();
                globalThis.began = "yes";
            } catch (error) {
                globalThis.began = String(error);
            }
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.interval").await,
            "undefined",
            "엔진이 setInterval 을 심었다면 이 시험의 전제가 바뀐 것이다"
        );
        assert_eq!(
            script.probe("globalThis.began").await,
            "yes",
            "티커가 시작되지 않는다"
        );
    }

    #[tokio::test]
    async fn a_class_advances_and_keeps_its_roots() {
        let script = probing(
            r#"
            import * as job from "./lib/game/job.js";
            import * as skill from "./lib/game/skill.js";
            import { blank, normalize, stats } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/jobs.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/npcs.js";

            const sword = registry.get("npc", "무명");
            const root = registry.get("npc", "뿌리");

            const s = blank("1", "손", "전사");
            s.level = 9; s.gold = 10000;
            globalThis.early = [job.options(s).length, job.advance(s, "기사", [sword]).ok, job.ahead(s)?.at].join(",");

            s.level = 10;
            globalThis.open = job.options(s).map((one) => one.id).join("/");
            globalThis.wrong = String(job.advance(s, "암살자", [sword]).ok);
            /// 아무 데서나 되면 여정이 아니다
            globalThis.nowhere = String(job.advance(s, "기사", []).ok);
            globalThis.wrongGuide = String(job.advance(s, "기사", [root]).ok);
            const hp0 = stats(s).maxHp;
            const done = job.advance(s, "기사", [sword]);
            globalThis.went = [done.ok, s.class, job.lineage(s).join(">"), s.gold, stats(s).maxHp > hp0].join(",");

            /// 앞 직업 재주는 남고, 새 직업 재주가 열린다
            const now = skill.known(s);
            globalThis.roots = [now.includes("방패베기"), now.includes("돌진"), now.includes("수호"), now.includes("반격")].join(",");

            /// 되돌릴 수 없다
            globalThis.back = String(job.advance(s, "전사", [sword]).ok);

            /// 3차는 뿌리 앞에서만
            s.level = 20; s.gold = 10000;
            globalThis.thirdWrong = String(job.advance(s, "성기사", [sword]).ok);
            globalThis.third = [job.options(s).map((one) => one.id).join("/"), job.advance(s, "성기사", [root]).ok, job.lineage(s).length, String(job.ahead(s)?.at)].join(",");

            /// 옛 시트
            const old = normalize({ id: "2", name: "옛", class: "도적", level: 12, gold: 1, skills: ["급습"] });
            globalThis.legacy = [old.lineage.join(">"), skill.known(old).includes("급습")].join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.early").await,
            "0,false,10",
            "레벨이 안 되는데 길이 열렸다"
        );
        assert_eq!(script.probe("globalThis.open").await, "기사/광전사");
        assert_eq!(
            script.probe("globalThis.wrong").await,
            "false",
            "남의 길로 갔다"
        );
        assert_eq!(
            script.probe("globalThis.nowhere").await,
            "false",
            "아무 데서나 전직했다"
        );
        assert_eq!(
            script.probe("globalThis.wrongGuide").await,
            "false",
            "엉뚱한 사람 앞에서 전직했다"
        );
        assert_eq!(
            script.probe("globalThis.thirdWrong").await,
            "false",
            "2차 안내자에게 3차를 했다"
        );
        assert_eq!(
            script.probe("globalThis.went").await,
            "true,기사,전사>기사,9500,true",
            "전직이 제대로 안 됐다"
        );
        assert_eq!(
            script.probe("globalThis.roots").await,
            "true,true,true,false",
            "앞 재주가 사라졌거나 뒤 재주가 미리 열렸다"
        );
        assert_eq!(script.probe("globalThis.back").await, "false", "되돌아갔다");
        assert_eq!(
            script.probe("globalThis.third").await,
            "성기사,true,3,25",
            "3차 뒤에 4차가 보여야 한다"
        );
        assert_eq!(script.probe("globalThis.legacy").await, "도적,true");
    }

    #[tokio::test]
    async fn a_high_land_warns_instead_of_locking() {
        let script = probing(
            r#"
            import * as world from "./lib/game/world.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";

            globalThis.onMessage = async () => {
                const s = blank("1", "손", "도적");
                s.level = 3; s.where = "안개늪";
                const gone = await world.travel(s, "서리고개");
                globalThis.went = [gone.ok, gone.risky, s.where].join(",");

                const there = world.foesIn("서리고개", 3).map((one) => one.level);
                globalThis.big = String(Math.max(...there) > 5);

                /// 고향 밖에서는 드물다
                globalThis.rare = [world.homely({ where: ["서리고개", "어스름숲"] }, "서리고개"), world.homely({ where: ["서리고개", "어스름숲"] }, "어스름숲")].join(",");
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.went").await,
            "true,true,서리고개",
            "높은 땅이 잠겨 있거나 경고가 없다"
        );
        assert_eq!(
            script.probe("globalThis.big").await,
            "true",
            "높은 땅에서 낮은 것만 만난다"
        );
        assert_eq!(script.probe("globalThis.rare").await, "1,0.25");
    }

    #[tokio::test]
    async fn rebirth_resets_the_body_and_hardens_the_world() {
        let script = probing(
            r#"
            import * as rebirth from "./lib/game/rebirth.js";
            import { blank, CEILING } from "./lib/game/sheet.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/classes.js";
            import "./lib/content/jobs.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";

            const s = blank("1", "손", "전사");
            s.level = 20; s.gold = 777; s.titles = ["백전"]; s.title = "백전"; s.lineage = ["전사", "기사"]; s.class = "기사";
            s.quests = { "슬라임 청소": { got: 1, cleared: false } };
            globalThis.early = String(rebirth.reborn(s).ok);

            s.level = CEILING;
            const done = rebirth.reborn(s);
            globalThis.after = [done.ok, s.rebirth, s.level, s.exp, Object.keys(s.quests).length, s.gold, s.titles.join("/"), s.class, s.lineage.join(">")].join(",");
            globalThis.told = String(rebirth.lost().some((line) => line.includes("돈")) && !rebirth.kept().includes("돈"));
            globalThis.badge = rebirth.badge(s);

            const slime = registry.get("monster", "슬라임");
            const hard = rebirth.harden(slime, 2);
            globalThis.hard = [hard.hp > slime.hp * 1.6, hard.power > slime.power * 1.6, hard.exp > slime.exp, Object.isFrozen(slime)].join(",");
            globalThis.soft = String(rebirth.harden(slime, 0) === slime);
            globalThis.among = String(rebirth.among([{ rebirth: 0 }, { rebirth: 3 }, {}]));
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.early").await,
            "false",
            "천장 전에 환생했다"
        );
        assert_eq!(
            script.probe("globalThis.after").await,
            "true,1,1,0,0,40,백전,기사,전사>기사",
            "남을 것이 사라졌거나 사라질 것이 남았다"
        );
        assert_eq!(
            script.probe("globalThis.told").await,
            "true",
            "돈이 사라진다고 말하지 않는다"
        );
        assert_eq!(script.probe("globalThis.badge").await, "✦1");
        assert_eq!(
            script.probe("globalThis.hard").await,
            "true,true,true,true",
            "세계가 안 세졌거나 정의를 건드렸다"
        );
        assert_eq!(script.probe("globalThis.soft").await, "true");
        assert_eq!(script.probe("globalThis.among").await, "3");
    }

    #[tokio::test]
    async fn the_deep_opens_only_to_the_reborn() {
        let script = probing(
            r#"
            import * as world from "./lib/game/world.js";
            import * as job from "./lib/game/job.js";
            import { blank, CEILING } from "./lib/game/sheet.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/classes.js";
            import "./lib/content/jobs.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";
            import "./lib/content/npcs.js";
            import "./lib/content/areas.js";

            globalThis.onMessage = async () => {
                const s = blank("1", "손", "전사");
                s.level = CEILING; s.gold = 99999; s.where = "심연바닥"; s.class = "성기사"; s.lineage = ["전사", "기사", "성기사"];
                const faceless = registry.get("npc", "얼굴없는자");

                const shut = await world.travel(s, "심연회랑");
                const noJob = job.advance(s, "수호성인", [faceless]);
                globalThis.shut = [shut.ok, s.where, noJob.ok].join(",");

                s.rebirth = 1;
                const open = await world.travel(s, "심연회랑");
                const yesJob = job.advance(s, "수호성인", [faceless]);
                globalThis.open = [open.ok, s.where, yesJob.ok, s.class, job.lineage(s).length].join(",");

                globalThis.guide = [faceless?.where, registry.get("region", faceless?.where)?.area, job.guideFor(4).length].join(",");
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.shut").await,
            "false,심연바닥,false",
            "환생 없이 문이 열렸다"
        );
        assert_eq!(
            script.probe("globalThis.open").await,
            "true,심연회랑,true,수호성인,4",
            "환생했는데 문이 안 열린다"
        );
        assert_eq!(
            script.probe("globalThis.guide").await,
            "그림자시장,심연 아래,1"
        );
    }

    #[tokio::test]
    async fn a_deleted_character_is_gone_and_can_be_remade() {
        let script = probing(
            r#"
            import * as store from "./lib/core/store.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";

            globalThis.onMessage = async () => {
                const held = {};
                const real = globalThis.fs;
                globalThis.fs = {
                    ...real,
                    exists: (p) => p.startsWith("data/") ? p in held : real.exists(p),
                    read: async (p) => p.startsWith("data/") ? held[p] : real.read(p),
                    write: async (p, c) => { if (p.startsWith("data/")) held[p] = c; else await real.write(p, c); return ""; },
                    list: async (p) => p.startsWith("data/") ? Object.keys(held).filter((k) => k.startsWith(p + "/")).map((k) => k.slice(p.length + 1)) : real.list(p),
                    remove: async (p) => { if (p.startsWith("data/")) delete held[p]; else await real.remove(p); return ""; },
                };

                const s = blank("z9", "지울사람", "전사");
                await store.keep(s);
                const before = (await store.find("z9")) !== null && "data/players/z9.json" in held;
                await store.forget("z9");
                const after = (await store.find("z9")) === null && !("data/players/z9.json" in held);
                const again = blank("z9", "새사람", "도적");
                await store.keep(again);
                const remade = (await store.find("z9"))?.name;
                globalThis.out = [before, after, remade].join(",");
                globalThis.fs = real;
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.out").await,
            "true,true,새사람",
            "지워지지 않았거나 다시 못 만든다"
        );
    }

    #[tokio::test]
    async fn free_lunch_commands_have_a_cooldown() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/commands/adventure.js";
            import "./lib/commands/character.js";

            globalThis.gaps = ["탐색", "행동", "휴식"]
                .map((id) => `${id}:${(registry.get("command", id)?.cooldown ?? 0) >= 30}`)
                .join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.gaps").await,
            "탐색:true,행동:true,휴식:true",
            "무한 자판기가 있다"
        );
    }

    #[tokio::test]
    async fn giving_moves_things_between_people_in_the_same_place() {
        let script = probing(
            r#"
            import { findHere } from "./lib/game/people.js";
            import * as store from "./lib/core/store.js";
            import { blank, carry, count } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";

            globalThis.onMessage = async () => {
                const held = {};
                const real = globalThis.fs;
                globalThis.fs = {
                    ...real,
                    exists: (p) => p.startsWith("data/") ? p in held : real.exists(p),
                    read: async (p) => p.startsWith("data/") ? held[p] : real.read(p),
                    write: async (p, c) => { if (p.startsWith("data/")) held[p] = c; else await real.write(p, c); return ""; },
                    list: async (p) => p.startsWith("data/") ? Object.keys(held).filter((k) => k.startsWith(p + "/")).map((k) => k.slice(p.length + 1)) : real.list(p),
                    remove: async (p) => { if (p.startsWith("data/")) delete held[p]; else await real.remove(p); return ""; },
                };

                const a = blank("g1", "갑", "전사"); a.where = "마을"; carry(a, "은화석", 3);
                const b = blank("g2", "을", "도적"); b.where = "마을";
                const c = blank("g3", "병", "사제"); c.where = "쇠터";
                for (const s of [a, b, c]) await store.keep(s);

                const near = await findHere(a, "을");
                const far = await findHere(a, "병");
                const me = await findHere(a, "갑");
                globalThis.lookup = [String(near.found?.id), String(far.found), String(me.found)].join(",");

                const { drop } = await import("./lib/game/sheet.js");
                drop(a, "은화석", 2); carry(near.found, "은화석", 2);
                await store.keep(a); await store.keep(near.found);
                globalThis.moved = [count(a, "은화석"), count(await store.find("g2"), "은화석")].join(",");
                globalThis.fs = real;
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.lookup").await,
            "g2,null,null",
            "먼 사람이나 자기 자신을 찾았다"
        );
        assert_eq!(
            script.probe("globalThis.moved").await,
            "1,2",
            "준 것이 저쪽에 없다"
        );
    }

    #[tokio::test]
    async fn a_dungeon_chains_floors_on_one_panel_and_pays_at_the_bottom() {
        let script = probing(
            r#"
            import { sandbox } from "./test/memfs.js";
            import * as dungeon from "./lib/game/dungeon.js";
            import * as registry from "./lib/core/registry.js";
            import { blank, count } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/skills.js";
            import "./lib/content/effects.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/phases.js";
            import "./lib/content/orders.js";
            import "./lib/content/traits.js";

            globalThis.onMessage = async () => {
                const { restore } = sandbox();
                try {
                const all = registry.all("monster");
                const laid = dungeon.plan(all, 5);
                globalThis.shape = [laid.length, laid[laid.length - 1].boss === true, laid.every((one) => one.floor >= 1), laid[0].hp < laid[laid.length - 1].hp].join(",");
                globalThis.prize = [String(dungeon.prize(1).item?.id), String(dungeon.prize(14).item?.id), dungeon.prize(14).stuff].join(",");

                const s = blank("d1", "탐험", "전사"); s.level = 8; s.where = "마을";
                const said = [];
                const msg = { say: async (t) => { said.push(t); return "log-" + said.length; }, chat: { edit: async () => {} } };
                const soft = (id, floor) => ({ ...registry.get("monster", id), hp: 30, power: 2, traits: [], skills: [], floor });
                const out = await dungeon.descend(msg, s, "마을", [soft("슬라임", 1), soft("고블린", 2)]);
                globalThis.went = [out.cleared, out.reached, s.wins, s.dungeons, count(s, "별가루") > 0, count(s, "미궁검"), said.length].join(",");

                /// 중간에 지면 거기까지
                const t = blank("d2", "겁쟁", "마법사"); t.level = 1; t.where = "마을";
                const wall = { ...registry.get("monster", "드래곤"), floor: 2 };
                const lost = await dungeon.descend(msg, t, "마을", [soft("슬라임", 1), wall, soft("슬라임", 3)]);
                globalThis.lost = [lost.cleared, lost.reached, t.dungeons ?? 0].join(",");
                } finally { restore(); }
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.shape").await,
            "6,true,true,true",
            "층이 깊어져도 안 세지거나 마지막이 보스가 아니다"
        );
        assert_eq!(
            script.probe("globalThis.prize").await,
            "미궁의반지,미궁의외투,심연가루"
        );
        // say 는 입구 한 번 + 바닥 한 번 = 2. 층마다 새 판을 띄우면 4가 된다.
        assert_eq!(
            script.probe("globalThis.went").await,
            "true,2,2,1,true,1,2",
            "층이 한 판에 안 이어지거나 바닥 보상이 없다"
        );
        assert_eq!(
            script.probe("globalThis.lost").await,
            "false,1,0",
            "졌는데 바닥 보상을 받았다"
        );
    }

    #[tokio::test]
    async fn the_codex_reveals_loot_after_five_kills() {
        let script = probing(
            r#"
            import { progress, REVEAL } from "./lib/commands/codex.js";
            import * as registry from "./lib/core/registry.js";
            import { blank } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/areas.js";
            import "./lib/content/traits.js";

            globalThis.onMessage = async () => {
                const s = blank("1", "손", "전사");
                const said = [];
                const say = async (t) => said.push(t);
                const codex = registry.get("command", "도감");

                globalThis.empty = String(progress(s).known);
                await codex.run({ sheet: s, rest: "슬라임", say });
                s.kills = { 슬라임: REVEAL - 1 };
                await codex.run({ sheet: s, rest: "슬라임", say });
                s.kills = { 슬라임: REVEAL };
                await codex.run({ sheet: s, rest: "슬라임", say });
                await codex.run({ sheet: s, rest: "", say });

                globalThis.unseen = String(said[0].includes("아직"));
                globalThis.hidden = String(said[1].includes("1마리 더"));
                globalThis.shown = String(said[2].includes("슬라임젤리"));
                globalThis.total = [progress(s).known, progress(s).total === registry.all("monster").length].join(",");
                globalThis.book = String(said[3].includes("1/") && said[3].includes("들녘"));
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(script.probe("globalThis.empty").await, "0");
        assert_eq!(
            script.probe("globalThis.unseen").await,
            "true",
            "안 잡은 것을 보여준다"
        );
        assert_eq!(
            script.probe("globalThis.hidden").await,
            "true",
            "다섯 마리 전에 전리품이 보인다"
        );
        assert_eq!(
            script.probe("globalThis.shown").await,
            "true",
            "다섯 마리인데 전리품이 안 보인다"
        );
        assert_eq!(script.probe("globalThis.total").await, "1,true");
        assert_eq!(
            script.probe("globalThis.book").await,
            "true",
            "도감 목록이 땅별로 안 묶인다"
        );
    }

    #[tokio::test]
    async fn a_red_variant_is_the_same_kind_but_meaner() {
        let script = probing(
            r#"
            import { variant } from "./lib/game/world.js";
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/monsters.js";
            import "./lib/content/regions.js";
            import "./lib/content/items.js";
            import "./lib/content/recipes.js";
            import "./lib/content/phases.js";

            const slime = registry.get("monster", "슬라임");
            const red = variant(slime);
            globalThis.same = String(red.id === slime.id);
            globalThis.mean = [red.hp > slime.hp, red.power > slime.power, red.gold > slime.gold, red.variant === true].join(",");
            globalThis.name = red.name;
            globalThis.core = String(red.loot.some((drop) => drop.id === "붉은핵"));
            globalThis.frozen = String(Object.isFrozen(slime) && slime.variant === undefined);
            globalThis.craft = String(registry.get("recipe", "붉은반지")?.takes?.붉은핵 > 0 && !!registry.get("item", "붉은반지"));
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.same").await,
            "true",
            "변종이 다른 종이 됐다"
        );
        assert_eq!(script.probe("globalThis.mean").await, "true,true,true,true");
        assert_eq!(script.probe("globalThis.name").await, "붉은 슬라임");
        assert_eq!(
            script.probe("globalThis.core").await,
            "true",
            "붉은핵이 안 나온다"
        );
        assert_eq!(
            script.probe("globalThis.frozen").await,
            "true",
            "원본 정의를 건드렸다"
        );
        assert_eq!(
            script.probe("globalThis.craft").await,
            "true",
            "붉은핵을 쓸 데가 없다"
        );
    }

    #[tokio::test]
    async fn only_rooms_tagged_rpg_are_served() {
        let script = probing(
            r#"
            import { roomAllowed, TAG, titleOf } from "./lib/core/gate.js";

            globalThis.onMessage = async () => {
                globalThis.names = [`동네 ${TAG} 방`, TAG.toLowerCase(), `우리 ${TAG.toUpperCase()} 모임`, "친구들", "", undefined]
                    .map((title) => roomAllowed(title))
                    .join(",");
                globalThis.read = [
                    await titleOf({ title: async () => `던전 ${TAG}` }),
                    await titleOf({ title: async () => { throw new Error("끊김"); } }),
                    await titleOf({}),
                ].join("|");
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.names").await,
            "true,true,true,false,false,false"
        );
        assert!(
            script.probe("globalThis.read").await.ends_with("||"),
            "이름을 못 읽었는데 봇이 죽거나 열렸다"
        );
    }

    /// 약할수록 잘 잡히고, 보스는 안 잡히고, 배고프면 안 움직이고, 먹이면 움직인다.
    #[tokio::test]
    async fn a_pet_is_tamed_when_weak_and_fights_when_fed() {
        let script = probing(
            r#"
            import * as pet from "./lib/game/pet.js";
            import * as registry from "./lib/core/registry.js";
            import { blank, carry } from "./lib/game/sheet.js";
            import "./lib/content/classes.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/content/monsters.js";
            import "./lib/content/effects.js";

            const fox = registry.get("monster", "여우");
            const dragon = registry.get("monster", "드래곤");
            const weak = { def: fox, hp: 2, maxHp: 20 };
            const strong = { def: fox, hp: 20, maxHp: 20 };
            globalThis.odds = [pet.odds(weak) > pet.odds(strong), pet.odds({ def: dragon, hp: 1, maxHp: 100 })].join(",");

            const s = blank("1", "손", "궁수");
            pet.adopt(s, fox);
            globalThis.have = [s.pet.id, s.pet.hunger, s.tamed].join(",");

            const kit = { strike: (t, n) => { t.hp -= n; return n; }, apply: () => true, heal: () => 0 };
            const foe = { hp: 100, maxHp: 100, effects: [], guard: 0 };
            const fed = pet.bite({ sheet: s }, foe, kit);
            s.pet.hunger = 0;
            const starved = pet.bite({ sheet: s }, foe, kit);
            globalThis.bite = [fed.includes("▸"), starved.includes("배가 고파"), foe.hp < 100].join(",");

            carry(s, "약초", 1); carry(s, "들쥐가죽", 1);
            const plain = pet.feed(s, "약초");
            const loved = pet.feed(s, "들쥐가죽");
            globalThis.feed = [plain.ok, plain.loved, loved.ok, loved.loved, s.pet.hunger, s.pet.bond].join(",");
            globalThis.refuse = [pet.feed(s, "낡은검").ok, pet.feed(s, "약초").ok].join(",");

            const grown = pet.after(s, 400);
            globalThis.grow = [grown.levels > 0, s.pet.level > 1, s.pet.hunger].join(",");
        "#,
        )
        .await;

        assert_eq!(
            script.probe("globalThis.odds").await,
            "true,0",
            "약한 놈이 더 안 잡히거나 보스가 잡힌다"
        );
        assert_eq!(script.probe("globalThis.have").await, "여우,10,1");
        assert_eq!(
            script.probe("globalThis.bite").await,
            "true,true,true",
            "배고픈데 움직이거나 배불러도 안 움직인다"
        );
        assert_eq!(
            script.probe("globalThis.feed").await,
            "true,false,true,true,10,4",
            "먹이가 안 먹히거나 좋아하는 것을 모른다"
        );
        assert_eq!(
            script.probe("globalThis.refuse").await,
            "false,false",
            "장비를 먹이거나 없는 것을 먹였다"
        );
        assert_eq!(
            script.probe("globalThis.grow").await,
            "true,true,9",
            "싸우고도 안 자라거나 배가 안 꺼진다"
        );
    }

    /// 처음 고를 수 있는 건 처음 직업뿐이다. 성기사로 시작하면 전직이 있을 이유가 없다.
    #[tokio::test]
    async fn you_cannot_start_as_an_advanced_class() {
        let script = probing(
            r#"
            import * as registry from "./lib/core/registry.js";
            import "./lib/content/classes.js";
            import "./lib/content/jobs.js";
            import "./lib/content/skills.js";
            import "./lib/content/items.js";
            import "./lib/content/regions.js";
            import "./lib/commands/character.js";

            globalThis.onMessage = async () => {
                const held = {};
                const real = globalThis.fs;
                globalThis.fs = {
                    ...real,
                    exists: (p) => p.startsWith("data/") ? p in held : real.exists(p),
                    read: async (p) => p.startsWith("data/") ? held[p] : real.read(p),
                    write: async (p, c) => { if (p.startsWith("data/")) held[p] = c; else await real.write(p, c); return ""; },
                    list: async (p) => p.startsWith("data/") ? Object.keys(held).filter((k) => k.startsWith(p + "/")).map((k) => k.slice(p.length + 1)) : real.list(p),
                    remove: async (p) => { if (p.startsWith("data/")) delete held[p]; else await real.remove(p); return ""; },
                };

                const said = [];
                const make = registry.get("command", "생성");
                const run = (className) => make.run({ msg: {}, args: ["손", className], who: `w-${className}`, say: async (t) => said.push(t) });

                await run("성기사");
                await run("수호성인");
                await run("전사");
                globalThis.out = [
                    said[0].includes("HP"),
                    said[1].includes("HP"),
                    said[2].includes("마을에서 시작"),
                    said[0].includes("성기사"),
                ].join(",");
                globalThis.fs = real;
            };
        "#,
        )
        .await;

        script.call("onMessage", ()).await.unwrap();

        assert_eq!(
            script.probe("globalThis.out").await,
            "true,true,true,false",
            "전직 직업으로 시작할 수 있다"
        );
    }
}
