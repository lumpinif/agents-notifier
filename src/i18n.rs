use crate::config::{AnswerDetail, CliLanguage, PromptDetail};

#[derive(Debug, Clone, Copy)]
pub struct I18n {
    language: CliLanguage,
}

impl I18n {
    pub fn new(language: CliLanguage) -> Self {
        Self { language }
    }

    pub fn language(self) -> CliLanguage {
        self.language
    }

    pub fn text(self, key: Text) -> &'static str {
        match self.language {
            CliLanguage::English => key.english(),
            CliLanguage::SimplifiedChinese => key.simplified_chinese(),
        }
    }

    pub fn answer_detail(self, value: AnswerDetail) -> &'static str {
        match (self.language, value) {
            (_, AnswerDetail::Preview) => "Preview",
            (CliLanguage::English, AnswerDetail::Full) => "Full Answer",
            (CliLanguage::SimplifiedChinese, AnswerDetail::Full) => "完整回答",
        }
    }

    pub fn prompt_detail(self, value: PromptDetail) -> &'static str {
        match (self.language, value) {
            (CliLanguage::English, PromptDetail::Off) => "Off",
            (CliLanguage::SimplifiedChinese, PromptDetail::Off) => "关闭",
            (CliLanguage::English, PromptDetail::On) => "On",
            (CliLanguage::SimplifiedChinese, PromptDetail::On) => "开启",
        }
    }
}

impl Default for I18n {
    fn default() -> Self {
        Self::new(CliLanguage::English)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Text {
    LanguagePrompt,
    CurrentSuffix,
    RecommendedSuffix,
    SetupTitle,
    SetupDoesNotChangeAgentSettings,
    SetupInteractiveRequired,
    FirstStartStartingSetup,
    FirstStartSetupScope,
    AgentPrompt,
    ProviderPrompt,
    AnswerDetailPrompt,
    PromptDetailPrompt,
    ConfigCreated,
    ConfigUpdated,
    AgentField,
    AnswerDetailField,
    PromptDetailField,
    Configured,
    NotConfigured,
    NotUsed,
    AllDevices,
    AccountDefault,
    NtfyIntro1,
    NtfyIntro2,
    FeishuLarkIntro1,
    FeishuLarkIntro2,
    FeishuLarkIntro3,
    WebhookIntro1,
    WebhookIntro2,
    PushoverIntro1,
    PushoverIntro2,
    SlackIntro1,
    SlackIntro2,
    DiscordIntro1,
    DiscordIntro2,
    TelegramIntro1,
    TelegramIntro2,
    WhatsappIntro1,
    WhatsappIntro2,
    WeixinIntro1,
    WeixinIntro2,
    TeamsIntro1,
    TeamsIntro2,
    EmailIntro1,
    EmailIntro2,
    WeixinSetupMethodPrompt,
    WeixinScanQrOption,
    WeixinExistingTokenOption,
    WeixinTokenVerifying,
    WeixinTokenVerified,
    WeixinKeepRecipientPrompt,
    WeixinQrScanPrompt,
    WeixinQrScanned,
    WeixinQrExpired,
    WeixinQrConfirmed,
    WeixinActionHeading,
    WeixinActionStep1,
    WeixinActionStep2,
    WeixinActionDoNotType,
    WeixinActionDone,
    WeixinWaitingForMessage,
    ServiceSection,
    ServiceStatusAlreadyRunning,
    ServiceStatusRunning,
    ServiceStatusUpdatedAndRestarted,
    NotificationsSection,
    WatchedAgentsSection,
    PhoneSubscriptionSection,
    NextPhone,
    NtfyFinishStep1,
    NtfyFinishStep2,
    NtfyFinishStep3,
    NextFeishuLark,
    NextWebhook,
    NextPushover,
    NextSlack,
    NextDiscord,
    NextTelegram,
    NextWhatsapp,
    NextWeixin,
    NextTeams,
    NextEmail,
    SendTestPromptNtfy,
    SendTestPromptFeishuLark,
    SendTestPromptWebhook,
    SendTestPromptPushover,
    SendTestPromptSlack,
    SendTestPromptDiscord,
    SendTestPromptTelegram,
    SendTestPromptWhatsapp,
    SendTestPromptWeixin,
    SendTestPromptTeams,
    SendTestPromptEmail,
    TestSent,
    DidItArrive,
    SetupComplete,
    TestDidNotArrive,
    CheckProviderSettingsAndRetest,
    SendTestNow,
    Working,
    ServiceRunningButTestMissing,
    CheckProviderSettingsPrinted,
}

impl Text {
    fn english(self) -> &'static str {
        match self {
            Self::LanguagePrompt => "Language / 语言",
            Self::CurrentSuffix => "Current",
            Self::RecommendedSuffix => "Recommended",
            Self::SetupTitle => "Set up agent notifications",
            Self::SetupDoesNotChangeAgentSettings => {
                "It does not change Codex or other agent settings."
            }
            Self::SetupInteractiveRequired => {
                "`agents-notifier setup` must be run in an interactive terminal so you can choose an agent and a provider."
            }
            Self::FirstStartStartingSetup => "Starting setup now.",
            Self::FirstStartSetupScope => "Setup connects one agent to one notification provider.",
            Self::AgentPrompt => "Which agent should Agents Notifier watch?",
            Self::ProviderPrompt => "Where should Agents Notifier send notifications?",
            Self::AnswerDetailPrompt => "Answer detail",
            Self::PromptDetailPrompt => "Include your prompt?",
            Self::ConfigCreated => "Created",
            Self::ConfigUpdated => "Updated",
            Self::AgentField => "agent",
            Self::AnswerDetailField => "answer detail",
            Self::PromptDetailField => "prompt detail",
            Self::Configured => "configured",
            Self::NotConfigured => "not configured",
            Self::NotUsed => "not used",
            Self::AllDevices => "all devices",
            Self::AccountDefault => "account default",
            Self::NtfyIntro1 => {
                "ntfy sends notifications to your phone through a topic subscription."
            }
            Self::NtfyIntro2 => {
                "Install the ntfy app on your phone, then subscribe to the topic below."
            }
            Self::FeishuLarkIntro1 => {
                "Feishu/Lark sends notifications to a group through a custom bot webhook."
            }
            Self::FeishuLarkIntro2 => {
                "Add a custom bot to the target group, then paste its webhook URL."
            }
            Self::FeishuLarkIntro3 => {
                "If you enable security, use Signature Verification. Keyword security can block notifications unless every message contains your keyword."
            }
            Self::WebhookIntro1 => "Webhook sends every signal as JSON to your HTTPS endpoint.",
            Self::WebhookIntro2 => {
                "Use this for internal tools, automation platforms, or your own notification bridge."
            }
            Self::PushoverIntro1 => {
                "Pushover sends notifications to your phone and desktop through your Pushover account."
            }
            Self::PushoverIntro2 => {
                "Create a Pushover application, then paste its API token and your user or group key."
            }
            Self::SlackIntro1 => {
                "Slack sends notifications to one channel through an incoming webhook."
            }
            Self::SlackIntro2 => {
                "Create a Slack app, enable Incoming Webhooks, then paste the channel webhook URL."
            }
            Self::DiscordIntro1 => {
                "Discord sends notifications to one channel through a channel webhook."
            }
            Self::DiscordIntro2 => "Create a Discord channel webhook, then paste its webhook URL.",
            Self::TelegramIntro1 => {
                "Telegram sends notifications through a Telegram bot to one chat or channel."
            }
            Self::TelegramIntro2 => {
                "Create a bot with BotFather, add it to the target chat if needed, then paste the bot token and chat id."
            }
            Self::WhatsappIntro1 => {
                "WhatsApp sends notifications through the WhatsApp Business Platform Cloud API."
            }
            Self::WhatsappIntro2 => {
                "Use a WhatsApp Business phone number ID, a system user access token, and one recipient phone number."
            }
            Self::WeixinIntro1 => {
                "WeChat sends notifications through a personal WeChat iLink bot connection."
            }
            Self::WeixinIntro2 => {
                "This is personal WeChat through iLink, not WeChat Work and not WhatsApp Business."
            }
            Self::TeamsIntro1 => {
                "Microsoft Teams sends notifications to one chat or channel through an incoming webhook."
            }
            Self::TeamsIntro2 => {
                "Create a Teams workflow or incoming webhook, then paste its webhook URL."
            }
            Self::EmailIntro1 => {
                "Email SMTP sends notifications through your SMTP server as plain text email."
            }
            Self::EmailIntro2 => {
                "Use STARTTLS on port 587 unless your provider explicitly gives you port 465."
            }
            Self::WeixinSetupMethodPrompt => "How should Agents Notifier connect to WeChat?",
            Self::WeixinScanQrOption => "Scan WeChat QR code",
            Self::WeixinExistingTokenOption => "Paste existing iLink token",
            Self::WeixinTokenVerifying => "Verifying WeChat iLink token...",
            Self::WeixinTokenVerified => "Token verified.",
            Self::WeixinKeepRecipientPrompt => "Keep the current linked WeChat recipient?",
            Self::WeixinQrScanPrompt => "Scan this QR code with WeChat:",
            Self::WeixinQrScanned => "QR code scanned. Confirm the login in WeChat.",
            Self::WeixinQrExpired => "QR code expired. Fetching a new one...",
            Self::WeixinQrConfirmed => "WeChat QR login confirmed.",
            Self::WeixinActionHeading => "ACTION REQUIRED: Open WeChat now.",
            Self::WeixinActionStep1 => "Find the bot chat that appeared after login:",
            Self::WeixinActionStep2 => "Send this exact message in that WeChat chat:",
            Self::WeixinActionDoNotType => {
                "Do not type `hi` here in Terminal. Send it inside WeChat."
            }
            Self::WeixinActionDone => {
                "DONE? Press Enter only after you sent `hi` in the WeChat bot chat."
            }
            Self::WeixinWaitingForMessage => "Waiting for the WeChat bot message you just sent...",
            Self::ServiceSection => "Service",
            Self::ServiceStatusAlreadyRunning => "already running",
            Self::ServiceStatusRunning => "running",
            Self::ServiceStatusUpdatedAndRestarted => "updated and restarted",
            Self::NotificationsSection => "Notifications",
            Self::WatchedAgentsSection => "Watched agents",
            Self::PhoneSubscriptionSection => "Phone subscription",
            Self::NextPhone => "Next: connect your phone.",
            Self::NtfyFinishStep1 => "1. Open the ntfy app.",
            Self::NtfyFinishStep2 => "2. Add a subscription.",
            Self::NtfyFinishStep3 => "3. Server: https://ntfy.sh",
            Self::NextFeishuLark => "Next: check your Feishu/Lark group.",
            Self::NextWebhook => "Next: check your webhook receiver.",
            Self::NextPushover => "Next: check your Pushover devices.",
            Self::NextSlack => "Next: check your Slack channel.",
            Self::NextDiscord => "Next: check your Discord channel.",
            Self::NextTelegram => "Next: check your Telegram chat.",
            Self::NextWhatsapp => "Next: check the recipient WhatsApp chat.",
            Self::NextWeixin => "Next: check the linked WeChat chat.",
            Self::NextTeams => "Next: check your Microsoft Teams chat or channel.",
            Self::NextEmail => "Next: check the recipient email inbox.",
            Self::SendTestPromptNtfy => {
                "After your phone is subscribed, press Enter to send a test notification."
            }
            Self::SendTestPromptFeishuLark => {
                "Press Enter to send a test notification to the custom bot."
            }
            Self::SendTestPromptWebhook => {
                "Press Enter to send a test JSON payload to the webhook."
            }
            Self::SendTestPromptPushover => {
                "Press Enter to send a test notification through Pushover."
            }
            Self::SendTestPromptSlack => "Press Enter to send a test notification through Slack.",
            Self::SendTestPromptDiscord => {
                "Press Enter to send a test notification through Discord."
            }
            Self::SendTestPromptTelegram => {
                "Press Enter to send a test notification through Telegram."
            }
            Self::SendTestPromptWhatsapp => {
                "Press Enter to send a test notification through WhatsApp."
            }
            Self::SendTestPromptWeixin => "Press Enter to send a test notification through WeChat.",
            Self::SendTestPromptTeams => {
                "Press Enter to send a test notification through Microsoft Teams."
            }
            Self::SendTestPromptEmail => {
                "Press Enter to send a test notification through Email SMTP."
            }
            Self::TestSent => "Test notification sent.",
            Self::DidItArrive => "Did it arrive?",
            Self::SetupComplete => "Setup complete. Agent notifications can now be forwarded.",
            Self::TestDidNotArrive => {
                "The service is still running, but the test notification did not arrive."
            }
            Self::CheckProviderSettingsAndRetest => {
                "Check the provider settings in your config, then run this test again:"
            }
            Self::SendTestNow => "Send a test notification now?",
            Self::Working => {
                "Agents Notifier is working. Agent notifications can now be forwarded."
            }
            Self::ServiceRunningButTestMissing => {
                "The service is running, but the test notification did not arrive."
            }
            Self::CheckProviderSettingsPrinted => "Check the provider settings printed above.",
        }
    }

    fn simplified_chinese(self) -> &'static str {
        match self {
            Self::LanguagePrompt => "语言 / Language",
            Self::CurrentSuffix => "当前",
            Self::RecommendedSuffix => "推荐",
            Self::SetupTitle => "设置 agent 通知",
            Self::SetupDoesNotChangeAgentSettings => "不会修改 Codex 或其他 agent 的设置。",
            Self::SetupInteractiveRequired => {
                "`agents-notifier setup` 必须在可交互终端里运行，用来选择 agent 和通知 provider。"
            }
            Self::FirstStartStartingSetup => "现在开始设置。",
            Self::FirstStartSetupScope => "这次只连接一个 agent 和一个通知 provider。",
            Self::AgentPrompt => "Agents Notifier 要监听哪个 agent？",
            Self::ProviderPrompt => "通知要发到哪里？",
            Self::AnswerDetailPrompt => "回答内容长度",
            Self::PromptDetailPrompt => "是否包含你的 prompt？",
            Self::ConfigCreated => "已创建",
            Self::ConfigUpdated => "已更新",
            Self::AgentField => "agent",
            Self::AnswerDetailField => "回答内容",
            Self::PromptDetailField => "prompt",
            Self::Configured => "已配置",
            Self::NotConfigured => "未配置",
            Self::NotUsed => "不使用",
            Self::AllDevices => "所有设备",
            Self::AccountDefault => "账号默认",
            Self::NtfyIntro1 => "ntfy 通过 topic 订阅把通知发到你的手机。",
            Self::NtfyIntro2 => "先在手机上安装 ntfy app，再订阅下面这个 topic。",
            Self::FeishuLarkIntro1 => "飞书/Lark 通过群里的自定义 bot webhook 接收通知。",
            Self::FeishuLarkIntro2 => "把自定义 bot 加到目标群，然后粘贴 webhook URL。",
            Self::FeishuLarkIntro3 => "如果开启安全设置，请用签名校验。关键词安全可能拦截通知。",
            Self::WebhookIntro1 => "Webhook 会把每个 signal 作为 JSON 发到你的 HTTPS endpoint。",
            Self::WebhookIntro2 => "适合内部工具、自动化平台，或你自己的通知桥。",
            Self::PushoverIntro1 => "Pushover 会通过你的 Pushover 账号发到手机和桌面。",
            Self::PushoverIntro2 => {
                "创建一个 Pushover application，然后粘贴 API token 和 user/group key。"
            }
            Self::SlackIntro1 => "Slack 通过 incoming webhook 把通知发到一个 channel。",
            Self::SlackIntro2 => {
                "创建 Slack app，开启 Incoming Webhooks，然后粘贴 channel webhook URL。"
            }
            Self::DiscordIntro1 => "Discord 通过 channel webhook 把通知发到一个 channel。",
            Self::DiscordIntro2 => "创建 Discord channel webhook，然后粘贴 webhook URL。",
            Self::TelegramIntro1 => "Telegram 通过 bot 把通知发到一个 chat 或 channel。",
            Self::TelegramIntro2 => {
                "用 BotFather 创建 bot，必要时把 bot 加进目标 chat，然后粘贴 token 和 chat id。"
            }
            Self::WhatsappIntro1 => "WhatsApp 通过 WhatsApp Business Platform Cloud API 发送通知。",
            Self::WhatsappIntro2 => {
                "需要 phone number ID、system user access token 和一个接收手机号。"
            }
            Self::WeixinIntro1 => "WeChat 通过个人微信 iLink bot 连接发送通知。",
            Self::WeixinIntro2 => "这是个人微信 iLink，不是企业微信，也不是 WhatsApp Business。",
            Self::TeamsIntro1 => {
                "Microsoft Teams 通过 incoming webhook 把通知发到一个 chat 或 channel。"
            }
            Self::TeamsIntro2 => "创建 Teams workflow 或 incoming webhook，然后粘贴 webhook URL。",
            Self::EmailIntro1 => "Email SMTP 通过你的 SMTP server 发送纯文本邮件通知。",
            Self::EmailIntro2 => "默认用 587 端口 STARTTLS。只有服务商明确要求时才用 465。",
            Self::WeixinSetupMethodPrompt => "Agents Notifier 要怎么连接 WeChat？",
            Self::WeixinScanQrOption => "扫描微信二维码",
            Self::WeixinExistingTokenOption => "粘贴已有 iLink token",
            Self::WeixinTokenVerifying => "正在验证 WeChat iLink token...",
            Self::WeixinTokenVerified => "Token 验证通过。",
            Self::WeixinKeepRecipientPrompt => "保留当前绑定的 WeChat 接收人？",
            Self::WeixinQrScanPrompt => "用微信扫描这个二维码：",
            Self::WeixinQrScanned => "二维码已扫描。现在请在微信里确认登录。",
            Self::WeixinQrExpired => "二维码已过期。正在获取新的二维码...",
            Self::WeixinQrConfirmed => "WeChat 扫码登录已确认。",
            Self::WeixinActionHeading => "现在打开微信。",
            Self::WeixinActionStep1 => "找到登录后出现的 bot 对话：",
            Self::WeixinActionStep2 => "在那个微信对话里发送这条消息：",
            Self::WeixinActionDoNotType => "不要在终端里输入 `hi`。要发到微信里的 bot 对话。",
            Self::WeixinActionDone => "已发送？只有在 WeChat bot 对话里发出 `hi` 后，才按 Enter。",
            Self::WeixinWaitingForMessage => "正在等待你刚才发出的 WeChat bot 消息...",
            Self::ServiceSection => "Service",
            Self::ServiceStatusAlreadyRunning => "已在运行",
            Self::ServiceStatusRunning => "运行中",
            Self::ServiceStatusUpdatedAndRestarted => "已更新并重启",
            Self::NotificationsSection => "通知",
            Self::WatchedAgentsSection => "监听的 agents",
            Self::PhoneSubscriptionSection => "手机订阅",
            Self::NextPhone => "下一步：连接你的手机。",
            Self::NtfyFinishStep1 => "1. 打开 ntfy app。",
            Self::NtfyFinishStep2 => "2. 添加一个 subscription。",
            Self::NtfyFinishStep3 => "3. Server: https://ntfy.sh",
            Self::NextFeishuLark => "下一步：检查你的飞书/Lark 群。",
            Self::NextWebhook => "下一步：检查你的 webhook receiver。",
            Self::NextPushover => "下一步：检查你的 Pushover 设备。",
            Self::NextSlack => "下一步：检查你的 Slack channel。",
            Self::NextDiscord => "下一步：检查你的 Discord channel。",
            Self::NextTelegram => "下一步：检查你的 Telegram chat。",
            Self::NextWhatsapp => "下一步：检查接收方 WhatsApp chat。",
            Self::NextWeixin => "下一步：检查已绑定的 WeChat chat。",
            Self::NextTeams => "下一步：检查你的 Microsoft Teams chat 或 channel。",
            Self::NextEmail => "下一步：检查接收邮箱。",
            Self::SendTestPromptNtfy => "手机订阅完成后，按 Enter 发送测试通知。",
            Self::SendTestPromptFeishuLark => "按 Enter 给自定义 bot 发送测试通知。",
            Self::SendTestPromptWebhook => "按 Enter 给 webhook 发送测试 JSON payload。",
            Self::SendTestPromptPushover => "按 Enter 通过 Pushover 发送测试通知。",
            Self::SendTestPromptSlack => "按 Enter 通过 Slack 发送测试通知。",
            Self::SendTestPromptDiscord => "按 Enter 通过 Discord 发送测试通知。",
            Self::SendTestPromptTelegram => "按 Enter 通过 Telegram 发送测试通知。",
            Self::SendTestPromptWhatsapp => "按 Enter 通过 WhatsApp 发送测试通知。",
            Self::SendTestPromptWeixin => "按 Enter 通过 WeChat 发送测试通知。",
            Self::SendTestPromptTeams => "按 Enter 通过 Microsoft Teams 发送测试通知。",
            Self::SendTestPromptEmail => "按 Enter 通过 Email SMTP 发送测试通知。",
            Self::TestSent => "测试通知已发送。",
            Self::DidItArrive => "收到了吗？",
            Self::SetupComplete => "设置完成。Agent 通知现在可以转发。",
            Self::TestDidNotArrive => "Service 仍在运行，但测试通知没有到达。",
            Self::CheckProviderSettingsAndRetest => {
                "检查 config 里的 provider 设置，然后重新运行这个测试："
            }
            Self::SendTestNow => "现在发送一条测试通知？",
            Self::Working => "Agents Notifier 已正常工作。Agent 通知现在可以转发。",
            Self::ServiceRunningButTestMissing => "Service 正在运行，但测试通知没有到达。",
            Self::CheckProviderSettingsPrinted => "检查上面打印出来的 provider 设置。",
        }
    }
}
