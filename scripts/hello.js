globalThis.onMessage = async (msg) => {
    if (msg.text === "핑") {
        await msg.say("퐁");
    }

    if (msg.text === "누구") {
        await msg.say(`${msg.author.name} (${msg.author.id})`);
    }

    if (msg.text === "멤버") {
        const members = await msg.chat.members();
        await msg.say(`${members.length}명: ${members.map((m) => m.name).join(", ")}`);
    }

    if (msg.text === "방") {
        const rooms = await msg.chat.session().chats();
        await msg.say(`방 ${rooms.length}개`);
    }

    if (msg.text.startsWith("입장 ")) {
        const link = await msg.chat.session().openLink(msg.text.slice(3).trim());
        if (!link) {
            await msg.say("그런 링크가 없다");
            return;
        }
        const joined = await link.join();
        await joined.write(`${link.name} 들어왔다`);
    }

    if (msg.text.startsWith("!eval ")) {
        const code = msg.text.slice("!eval ".length);
        try {
            await msg.say(String(await eval(`(async () => { return ${code} })()`)));
        } catch (error) {
            await msg.say(String(error));
        }
    }
};

globalThis.onJoin = async (chat, members) => {
    for (const member of members) {
        await chat.write(`${member.name}님 어서오세요`);
    }
};

globalThis.onLeave = async (chat, members) => {
    for (const member of members) {
        await chat.write(`${member.name}님 안녕히`);
    }
};

globalThis.onMemberChange = (chat, joined, members) => {
    console.log(`멤버 ${joined ? "입장" : "퇴장"}: ${members.map((m) => m.name).join(", ")}`);
};

globalThis.onRead = (chat, userId, watermark) => console.log(`읽음 ${userId} → ${watermark}`);
globalThis.onReaction = (chat, logId, type, content) => console.log(`반응 ${logId} ${type} ${content}`);
globalThis.onFeed = (chat, feedType) => console.log(`피드 ${feedType}`);
globalThis.onMetaChange = (chatId, type, content) => console.log(`메타 ${chatId} ${type} ${content}`);
globalThis.onSyncJoin = (chat) => console.log(`다른 기기 입장 ${chat.id}`);
globalThis.onLinkProfile = (chat, linkId) => console.log(`프로필 변경 ${linkId}`);
globalThis.onLeft = (chat) => console.log(`나감 ${chat.id}`);
globalThis.onLogin = (userId) => console.log(`로그인 ${userId}`);
globalThis.onListening = (seconds) => console.log(`대기 시작, 핑 ${seconds}초`);
globalThis.onKicked = () => console.log("밀려남");
globalThis.onMoved = (method) => console.log(`서버 이동 ${method}`);
globalThis.onPush = (method) => console.log(`푸시 ${method}`);
globalThis.onConnect = () => console.log("연결됨");
globalThis.onClose = (reason) => console.log(`종료 ${reason}`);
