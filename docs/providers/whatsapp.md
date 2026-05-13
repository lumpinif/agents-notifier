# WhatsApp

中文文档：[whatsapp.zh-CN.md](whatsapp.zh-CN.md)

Use WhatsApp when you want Agents Router updates sent to one WhatsApp user through the WhatsApp Business Platform Cloud API.

Agents Router sends non-template text messages with the Cloud API. This is a notification sender, not a two-way WhatsApp chat bot.

## Official Links

- [WhatsApp Business Platform](https://developers.facebook.com/docs/whatsapp)
- [Cloud API Get Started](https://developers.facebook.com/documentation/business-messaging/whatsapp/get-started)
- [Cloud API](https://developers.facebook.com/docs/whatsapp/cloud-api)

## What You Need

- A Meta developer app connected to a WhatsApp Business Account.
- A WhatsApp Business phone number ID.
- A system user access token that can send WhatsApp messages.
- One recipient phone number with country code and digits only.
- Agents Router installed.

## 1. Prepare WhatsApp Cloud API Access

In Meta's WhatsApp Business Platform setup, get:

```text
Phone number ID
System user access token
Recipient WhatsApp phone number
```

The Cloud API sends text messages with this request shape:

```text
POST https://graph.facebook.com/v23.0/<PHONE_NUMBER_ID>/messages
Authorization: Bearer <ACCESS_TOKEN>
Content-Type: application/json
```

Agents Router sends:

```json
{
  "messaging_product": "whatsapp",
  "recipient_type": "individual",
  "to": "15551234567",
  "type": "text",
  "text": {
    "body": "Agents Router message"
  }
}
```

## 2. Understand the 24-Hour Window

WhatsApp non-template messages are for the customer service window. In practice, this means the recipient normally needs to have messaged your WhatsApp Business account within the allowed window.

Agents Router does not send WhatsApp template messages in this provider. It only sends plain notification text.

## 3. Connect Agents Router

Run:

```bash
agents-router setup
```

Choose:

```text
WhatsApp
```

Paste the access token, phone number ID, and recipient phone number.

Agents Router stores the provider, starts the local service, and sends a test message through the same service route used by real agent events.

## Answer Detail

Agents Router fixes answer detail to `Preview` for WhatsApp.

WhatsApp notifications should stay short. Agents Router uses a 4096-character local guard for WhatsApp text bodies and fails before sending if a formatted notification is too long.

## Prompt Detail

Agents Router disables prompt detail for WhatsApp.

Prompts can be long and private, so Agents Router keeps prompts out of WhatsApp notifications.

## Manual Config

WhatsApp is configured in:

```text
~/.config/agents-router/config.toml
```

Simple config:

```toml
[[providers]]
id = "whatsapp"
type = "whatsapp"
access_token = "<your WhatsApp Cloud API access token>"
phone_number_id = "123456789012345"
recipient_phone_number = "15551234567"

[[routes]]
sources = ["codex_desktop"]
providers = ["whatsapp"]

[[routes]]
sources = ["agents_router"]
providers = ["whatsapp"]
```

Advanced: `access_token_env` is supported, but only use it when the environment variable is visible to the running local service. For normal setup, `access_token` is simpler and more predictable.

The running service automatically reloads valid config changes. If it is not running, start it:

```bash
agents-router start
```

## Limits

Agents Router uses Graph API version `v23.0`.

`phone_number_id` must be digits only. `recipient_phone_number` must be 7 to 15 digits with no `+`, spaces, or punctuation.

If a formatted WhatsApp notification is too long, Agents Router fails the WhatsApp delivery before sending. It does not silently cut your message.

## If It Does Not Show Up

Check these first:

- The access token is valid and not expired.
- The phone number ID is exact.
- The recipient phone number includes country code and digits only.
- The recipient is allowed by your WhatsApp Business Platform setup.
- You are inside the customer service window for non-template messages.
- If you use `access_token_env`, the environment variable is visible to the running service.
- The local service is running:

```bash
agents-router status
```
