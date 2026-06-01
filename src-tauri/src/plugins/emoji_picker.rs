use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct EmojiPickerPlugin;

// Top emojis with search keywords (emoji, name, keywords...)
static EMOJIS: &[(&str, &str, &[&str])] = &[
    ("😀", "grinning face", &["happy", "smile", "joy", "grin"]),
    (
        "😂",
        "face with tears of joy",
        &["laugh", "lol", "funny", "cry", "haha"],
    ),
    (
        "🥹",
        "holding back tears",
        &["grateful", "moved", "touched", "cry"],
    ),
    ("😭", "loudly crying face", &["sad", "cry", "sob", "tears"]),
    ("😍", "heart eyes", &["love", "crush", "adore", "beautiful"]),
    (
        "🥰",
        "smiling face with hearts",
        &["love", "affection", "cute", "adore"],
    ),
    (
        "😘",
        "kissing face with heart",
        &["kiss", "love", "mwah", "blowing"],
    ),
    (
        "🤣",
        "rolling on floor laughing",
        &["rofl", "laugh", "funny", "lol"],
    ),
    (
        "😊",
        "smiling face with eyes",
        &["happy", "blush", "smile", "pleased"],
    ),
    (
        "🙃",
        "upside down face",
        &["sarcasm", "silly", "irony", "joking"],
    ),
    ("😏", "smirking face", &["smug", "sly", "flirt", "smirk"]),
    (
        "😒",
        "unamused face",
        &["annoyed", "meh", "bored", "unimpressed"],
    ),
    (
        "😔",
        "pensive face",
        &["sad", "thoughtful", "regret", "sorry"],
    ),
    ("😢", "crying face", &["sad", "tear", "cry", "unhappy"]),
    (
        "😤",
        "face with steam",
        &["frustrated", "angry", "annoyed", "huffing"],
    ),
    ("😡", "pouting face", &["angry", "mad", "rage", "red"]),
    (
        "🤬",
        "face with symbols on mouth",
        &["swear", "angry", "cursing", "mad"],
    ),
    (
        "🤯",
        "exploding head",
        &["mind blown", "shocked", "wow", "surprised"],
    ),
    (
        "😱",
        "screaming in fear",
        &["scared", "shocked", "oh no", "horror"],
    ),
    (
        "😨",
        "fearful face",
        &["scared", "fear", "nervous", "anxiety"],
    ),
    (
        "😰",
        "anxious face with sweat",
        &["worried", "nervous", "stress", "sweat"],
    ),
    (
        "😓",
        "downcast face with sweat",
        &["tired", "hard work", "sweat", "relief"],
    ),
    (
        "🤗",
        "hugging face",
        &["hug", "warm", "embrace", "friendly"],
    ),
    (
        "🤔",
        "thinking face",
        &["think", "ponder", "hmm", "question"],
    ),
    (
        "🤐",
        "zipper mouth face",
        &["quiet", "secret", "silence", "shh"],
    ),
    ("😴", "sleeping face", &["sleep", "tired", "zzz", "nap"]),
    (
        "🤒",
        "face with thermometer",
        &["sick", "ill", "fever", "unwell"],
    ),
    (
        "🤢",
        "nauseated face",
        &["sick", "disgusted", "gross", "nausea"],
    ),
    (
        "🤮",
        "vomiting face",
        &["sick", "gross", "disgusting", "barf"],
    ),
    (
        "🥵",
        "hot face",
        &["hot", "overheated", "sweating", "fever"],
    ),
    ("🥶", "cold face", &["cold", "freezing", "chilly", "frost"]),
    (
        "😇",
        "smiling face with halo",
        &["angel", "innocent", "good", "pure"],
    ),
    (
        "🥳",
        "partying face",
        &["party", "celebrate", "birthday", "fun"],
    ),
    (
        "🤩",
        "star-struck",
        &["wow", "amazing", "star", "excited", "star eyes"],
    ),
    (
        "😎",
        "smiling face with sunglasses",
        &["cool", "chill", "awesome", "sunglasses"],
    ),
    (
        "🧐",
        "face with monocle",
        &["curious", "inspect", "detective", "hmm"],
    ),
    ("🤓", "nerd face", &["nerd", "geek", "smart", "glasses"]),
    ("👍", "thumbs up", &["like", "good", "yes", "approve", "ok"]),
    ("👎", "thumbs down", &["dislike", "bad", "no", "disapprove"]),
    (
        "👏",
        "clapping hands",
        &["applause", "clap", "bravo", "celebrate"],
    ),
    (
        "🙌",
        "raising hands",
        &["celebrate", "praise", "hooray", "yay"],
    ),
    (
        "🤝",
        "handshake",
        &["deal", "agree", "partnership", "shake"],
    ),
    (
        "🙏",
        "folded hands",
        &["please", "pray", "thank you", "namaste"],
    ),
    ("👊", "oncoming fist", &["fist", "punch", "bump", "fight"]),
    (
        "✊",
        "raised fist",
        &["fist", "power", "solidarity", "strong"],
    ),
    (
        "🤞",
        "crossed fingers",
        &["luck", "hope", "wish", "fingers"],
    ),
    ("✌️", "victory hand", &["peace", "victory", "two", "v sign"]),
    ("🤟", "love you gesture", &["love", "rock", "ily", "hand"]),
    (
        "🤙",
        "call me hand",
        &["call", "hang loose", "phone", "shaka"],
    ),
    (
        "👆",
        "backhand pointing up",
        &["up", "point", "above", "direction"],
    ),
    (
        "👇",
        "backhand pointing down",
        &["down", "point", "below", "direction"],
    ),
    (
        "👈",
        "backhand pointing left",
        &["left", "point", "direction"],
    ),
    (
        "👉",
        "backhand pointing right",
        &["right", "point", "direction"],
    ),
    (
        "☝️",
        "index pointing up",
        &["one", "point", "up", "attention"],
    ),
    (
        "🫵",
        "index pointing at viewer",
        &["you", "point", "choose", "pick"],
    ),
    (
        "💪",
        "flexed biceps",
        &["strong", "muscle", "flex", "workout"],
    ),
    (
        "🦾",
        "mechanical arm",
        &["robot", "strong", "bionic", "prosthetic"],
    ),
    (
        "🖐️",
        "hand with fingers splayed",
        &["five", "stop", "hi", "hand"],
    ),
    ("✋", "raised hand", &["stop", "high five", "wait", "raise"]),
    (
        "👋",
        "waving hand",
        &["hello", "hi", "wave", "bye", "goodbye"],
    ),
    (
        "🤚",
        "raised back of hand",
        &["stop", "hand", "raise", "back"],
    ),
    (
        "🖖",
        "vulcan salute",
        &["star trek", "spock", "vulcan", "live long"],
    ),
    ("💅", "nail polish", &["nails", "fancy", "done", "sassy"]),
    ("🫶", "heart hands", &["love", "heart", "hands", "care"]),
    (
        "❤️",
        "red heart",
        &["love", "heart", "romance", "affection"],
    ),
    ("🧡", "orange heart", &["love", "heart", "orange", "warm"]),
    ("💛", "yellow heart", &["love", "heart", "yellow", "happy"]),
    ("💚", "green heart", &["love", "heart", "green", "nature"]),
    ("💙", "blue heart", &["love", "heart", "blue", "trust"]),
    (
        "💜",
        "purple heart",
        &["love", "heart", "purple", "royalty"],
    ),
    ("🖤", "black heart", &["love", "heart", "black", "dark"]),
    ("🤍", "white heart", &["love", "heart", "white", "pure"]),
    (
        "💔",
        "broken heart",
        &["heartbreak", "sad", "loss", "rejection"],
    ),
    (
        "❤️‍🔥",
        "heart on fire",
        &["love", "passion", "burning", "intense"],
    ),
    (
        "💯",
        "hundred points",
        &["perfect", "100", "agree", "score"],
    ),
    ("💥", "collision", &["boom", "explosion", "pow", "bang"]),
    (
        "✨",
        "sparkles",
        &["sparkle", "shine", "glitter", "magic", "star"],
    ),
    ("🔥", "fire", &["hot", "fire", "flame", "lit", "trending"]),
    ("💫", "dizzy", &["star", "spin", "dizziness", "sparkle"]),
    ("⭐", "star", &["star", "favorite", "rating", "yellow"]),
    ("🌟", "glowing star", &["star", "shine", "special", "glow"]),
    (
        "🎉",
        "party popper",
        &["party", "celebrate", "congrats", "tada"],
    ),
    (
        "🎊",
        "confetti ball",
        &["party", "celebrate", "confetti", "festival"],
    ),
    (
        "🎈",
        "balloon",
        &["balloon", "birthday", "party", "celebration"],
    ),
    (
        "🎁",
        "wrapped gift",
        &["gift", "present", "birthday", "wrap"],
    ),
    (
        "🏆",
        "trophy",
        &["award", "win", "champion", "trophy", "first"],
    ),
    (
        "🥇",
        "first place medal",
        &["gold", "win", "first", "champion"],
    ),
    (
        "🎯",
        "bullseye",
        &["target", "hit", "accurate", "goal", "dart"],
    ),
    (
        "🚀",
        "rocket",
        &["launch", "space", "fast", "rocket", "ship"],
    ),
    (
        "💡",
        "light bulb",
        &["idea", "light", "bulb", "bright", "innovation"],
    ),
    (
        "🔑",
        "key",
        &["key", "access", "password", "unlock", "lock"],
    ),
    ("🔒", "locked", &["lock", "secure", "password", "private"]),
    ("🔓", "unlocked", &["unlock", "open", "access", "free"]),
    (
        "⚡",
        "lightning",
        &["lightning", "fast", "power", "electric", "zap"],
    ),
    (
        "🌈",
        "rainbow",
        &["rainbow", "colorful", "hope", "gay", "pride"],
    ),
    ("☀️", "sun", &["sun", "sunny", "warm", "day", "bright"]),
    (
        "🌙",
        "crescent moon",
        &["moon", "night", "sleep", "crescent"],
    ),
    (
        "⭕",
        "hollow red circle",
        &["circle", "zero", "correct", "o"],
    ),
    ("❌", "cross mark", &["wrong", "no", "cancel", "x", "error"]),
    (
        "✅",
        "check mark button",
        &["done", "yes", "correct", "check", "ok"],
    ),
    ("❓", "question mark", &["question", "ask", "help", "what"]),
    (
        "❗",
        "exclamation mark",
        &["important", "alert", "warning", "attention"],
    ),
    ("⚠️", "warning", &["warning", "caution", "alert", "danger"]),
    ("🔴", "red circle", &["red", "circle", "stop", "dot"]),
    ("🟠", "orange circle", &["orange", "circle", "dot"]),
    ("🟡", "yellow circle", &["yellow", "circle", "dot"]),
    (
        "🟢",
        "green circle",
        &["green", "circle", "dot", "go", "online"],
    ),
    ("🔵", "blue circle", &["blue", "circle", "dot"]),
    ("🟣", "purple circle", &["purple", "circle", "dot"]),
    ("⚫", "black circle", &["black", "circle", "dot"]),
    ("⚪", "white circle", &["white", "circle", "dot"]),
    (
        "💻",
        "laptop",
        &["computer", "laptop", "work", "code", "tech"],
    ),
    (
        "🖥️",
        "desktop computer",
        &["computer", "desktop", "monitor", "screen"],
    ),
    (
        "📱",
        "mobile phone",
        &["phone", "mobile", "smartphone", "cell"],
    ),
    ("⌨️", "keyboard", &["keyboard", "type", "input", "keys"]),
    (
        "🖱️",
        "computer mouse",
        &["mouse", "cursor", "click", "pointer"],
    ),
    ("💾", "floppy disk", &["save", "disk", "storage", "floppy"]),
    ("💿", "optical disk", &["cd", "disk", "music", "data"]),
    ("📷", "camera", &["camera", "photo", "picture", "snapshot"]),
    (
        "📸",
        "camera with flash",
        &["camera", "flash", "photo", "selfie"],
    ),
    ("🎵", "musical note", &["music", "note", "song", "tune"]),
    ("🎶", "musical notes", &["music", "notes", "song", "melody"]),
    ("🎤", "microphone", &["mic", "microphone", "sing", "record"]),
    (
        "🎧",
        "headphone",
        &["headphones", "music", "listen", "audio"],
    ),
    ("📺", "television", &["tv", "television", "watch", "screen"]),
    ("📻", "radio", &["radio", "listen", "broadcast", "music"]),
    (
        "🎮",
        "video game",
        &["game", "gaming", "controller", "play", "xbox"],
    ),
    ("🕹️", "joystick", &["game", "joystick", "arcade", "control"]),
    (
        "📚",
        "books",
        &["books", "read", "study", "learn", "library"],
    ),
    ("📖", "open book", &["book", "read", "open", "story"]),
    ("📝", "memo", &["write", "note", "memo", "pencil", "edit"]),
    ("✏️", "pencil", &["write", "pencil", "edit", "draw"]),
    ("📌", "pushpin", &["pin", "mark", "location", "attach"]),
    ("📎", "paperclip", &["attach", "clip", "file", "paperclip"]),
    ("📧", "email", &["email", "mail", "message", "envelope"]),
    (
        "📨",
        "incoming envelope",
        &["email", "mail", "inbox", "receive"],
    ),
    ("📤", "outbox tray", &["send", "upload", "outbox", "email"]),
    (
        "📥",
        "inbox tray",
        &["receive", "download", "inbox", "email"],
    ),
    ("📦", "package", &["package", "box", "delivery", "parcel"]),
    ("🗑️", "wastebasket", &["trash", "delete", "bin", "waste"]),
    (
        "🔍",
        "magnifying glass",
        &["search", "find", "zoom", "investigate"],
    ),
    (
        "🔧",
        "wrench",
        &["tool", "wrench", "fix", "repair", "settings"],
    ),
    ("⚙️", "gear", &["settings", "gear", "config", "options"]),
    ("🔨", "hammer", &["hammer", "build", "tool", "construct"]),
    (
        "🪛",
        "screwdriver",
        &["screwdriver", "tool", "fix", "repair"],
    ),
    ("🧲", "magnet", &["magnet", "attract", "pull", "metal"]),
    (
        "💰",
        "money bag",
        &["money", "cash", "rich", "wealth", "bag"],
    ),
    ("💳", "credit card", &["card", "payment", "credit", "buy"]),
    (
        "💎",
        "gem stone",
        &["diamond", "gem", "jewel", "precious", "luxury"],
    ),
    ("🏠", "house", &["home", "house", "building", "live"]),
    (
        "🏢",
        "office building",
        &["office", "work", "building", "company"],
    ),
    ("🌍", "earth", &["world", "earth", "globe", "international"]),
    ("🗺️", "world map", &["map", "world", "travel", "navigate"]),
    (
        "✈️",
        "airplane",
        &["plane", "fly", "travel", "airplane", "flight"],
    ),
    ("🚗", "car", &["car", "drive", "vehicle", "auto", "red"]),
    ("🚕", "taxi", &["taxi", "cab", "ride", "yellow"]),
    ("🍕", "pizza", &["pizza", "food", "eat", "slice", "italian"]),
    ("🍔", "hamburger", &["burger", "food", "eat", "fast food"]),
    ("🍟", "fries", &["fries", "food", "potato", "fast food"]),
    (
        "🍣",
        "sushi",
        &["sushi", "food", "japanese", "fish", "rice"],
    ),
    ("🍜", "steaming bowl", &["noodles", "ramen", "food", "soup"]),
    ("🍰", "shortcake", &["cake", "dessert", "birthday", "sweet"]),
    (
        "☕",
        "hot beverage",
        &["coffee", "tea", "hot", "drink", "morning"],
    ),
    ("🧋", "bubble tea", &["boba", "tea", "drink", "bubble"]),
    ("🍺", "beer mug", &["beer", "drink", "cheers", "pub"]),
    (
        "🍻",
        "clinking beer mugs",
        &["beer", "cheers", "drink", "celebrate"],
    ),
    ("🍷", "wine glass", &["wine", "drink", "cheers", "red"]),
    ("🐶", "dog face", &["dog", "puppy", "pet", "woof"]),
    ("🐱", "cat face", &["cat", "kitty", "pet", "meow"]),
    ("🐸", "frog", &["frog", "green", "ribbit", "jump"]),
    ("🦊", "fox", &["fox", "clever", "orange", "cunning"]),
    ("🐼", "panda", &["panda", "bear", "cute", "china"]),
    (
        "🌺",
        "hibiscus",
        &["flower", "tropical", "pink", "hibiscus"],
    ),
    ("🌻", "sunflower", &["sunflower", "flower", "yellow", "sun"]),
    (
        "🍀",
        "four leaf clover",
        &["lucky", "clover", "luck", "irish"],
    ),
    ("🌊", "wave", &["wave", "ocean", "sea", "water", "surf"]),
    ("🏔️", "mountain", &["mountain", "peak", "snow", "climb"]),
    (
        "🌲",
        "evergreen tree",
        &["tree", "forest", "green", "nature"],
    ),
    ("🕐", "one oclock", &["clock", "time", "hour", "one"]),
    (
        "⏰",
        "alarm clock",
        &["alarm", "clock", "wake", "time", "morning"],
    ),
    (
        "⏳",
        "hourglass",
        &["time", "wait", "hourglass", "sand", "timer"],
    ),
    ("📅", "calendar", &["calendar", "date", "schedule", "event"]),
];

fn score_emoji(name: &str, keywords: &[&str], query: &str) -> i32 {
    let query_lower = query.to_lowercase();
    if name.to_lowercase().starts_with(&query_lower) {
        return 90;
    }
    if name.to_lowercase().contains(&query_lower) {
        return 70;
    }
    for kw in keywords {
        if kw.starts_with(&query_lower) {
            return 80;
        }
        if kw.contains(&query_lower) {
            return 60;
        }
    }
    0
}

#[async_trait]
impl Plugin for EmojiPickerPlugin {
    fn name(&self) -> &str {
        "emoji_picker"
    }

    fn description(&self) -> &str {
        "Search and copy emojis (prefix: emoji <keyword>)"
    }

    fn keyword(&self) -> Option<&str> {
        Some("emoji ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let search = q
            .raw
            .strip_prefix("emoji ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let mut results: Vec<QueryResult> = EMOJIS
            .iter()
            .filter_map(|(emoji, name, keywords)| {
                let score = if search.is_empty() {
                    50 // show all when no query
                } else {
                    score_emoji(name, keywords, &search)
                };
                if score == 0 {
                    return None;
                }
                Some(QueryResult {
                    id: format!("emoji:{}", emoji),
                    title: format!("{} {}", emoji, name),
                    subtitle: Some(format!("Copy {} to clipboard", emoji)),
                    icon: Some(emoji.to_string()),
                    score,
                    action_type: "copy".to_string(),
                    action_data: emoji.to_string(),
                    source: None,
                })
            })
            .collect();

        results.sort_by_key(|r| std::cmp::Reverse(r.score));
        results.truncate(12);
        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "emoji_picker",
                "description": "Search emojis by keyword and return matching emojis with names",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Keyword to search emojis (e.g. happy, fire, heart)" }
                    },
                    "required": ["query"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let query = args["query"].as_str().unwrap_or("").trim().to_lowercase();
        if query.is_empty() {
            return "Error: 'query' parameter is required".to_string();
        }
        let results: Vec<_> = EMOJIS
            .iter()
            .filter_map(|(emoji, name, keywords)| {
                let s = score_emoji(name, keywords, &query);
                if s == 0 {
                    None
                } else {
                    Some((s, emoji, name))
                }
            })
            .collect();
        if results.is_empty() {
            return format!("No emojis found matching '{}'", query);
        }
        let mut results = results;
        results.sort_by_key(|x| std::cmp::Reverse(x.0));
        results.truncate(12);
        results
            .into_iter()
            .map(|(_, emoji, name)| format!("{} {}", emoji, name))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
