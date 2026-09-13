# Connect your AI

Open **Settings → AI accounts → Add AI account**, then choose a provider.

For **OpenAI** and **xAI**, choose either **Use my subscription** or **API key**. These are separate connections with separate sign-in and billing. Anthropic, Google and Custom currently use API keys inside Quantix; the provider screen explains the subscription limitation and links to the relevant documentation.

## Use a ChatGPT or Grok subscription

1. Choose the subscription method. Quantix prepares the required official client privately under `~/.quantix`.
2. Select **Sign in with browser**, then complete sign-in on the provider's official page. **Use a device code** is an alternative. Quantix stays on the current step until the provider confirms the result; you can cancel.
3. Choose a model from that account and select **Check subscription**. The check uses a small generic sample, without Tender content.
4. When the check passes, choose the account in your Tender's **AI setup**.

The subscription must include access to the chosen client and model. Quantix does not turn a ChatGPT/Grok session into an API key, switch to a paid API after failure, or promise unlimited usage. Existing saved sign-in is reused when the official client confirms it.

A subscription check stops after 180 seconds. Grok allows at most five worker rounds. Codex runs one client turn; its internal inference count and token limit are not exposed as a hard Quantix cap. Provider plan limits still apply.

For Grok, Quantix refreshes the account's allowance before checking. If included-only access cannot be confirmed, the panel shows the reason, **Refresh Grok usage**, and a link to Grok. In Grok, open **Settings → Usage** to review the account. Quantix does not change provider top-up settings. Provider-managed extras require an explicit choice under **More options** and renewed Tender approval; they do not bypass the subscription-check allowance requirement.

**More options** also contains account rename, refresh, sign-out, software repair/removal and account removal. Repairing software preserves the account's private sign-in; a new or changed identity requires another model check and Tender approval.

## Use an API key

1. Choose the API-key method and enter the provider's key inside Quantix. Keep keys out of documents and chat messages.
2. Choose a model. If a custom server does not list models, enter its documented model ID manually.
3. Select **Check connection**. Saved keys remain hidden; leave the field empty when keeping an existing key.

The direct API check allows two requests, 1,024 output tokens and a conservative 16,384-byte input allowance per request, with a 90-second deadline. Known pricing produces a displayed check allowance. Unknown pricing requires explicit consent; bounded requests do not establish a dollar cap. A check does not approve Tender spending.

## Continue or recover

A passed check proves that the selected account/model completed the tool and result exchange; it does not certify engineering accuracy. Changing account access or relevant settings can require another check. Quantix keeps the selected model and explains the next action after a failed check.

- **Sign-in needed:** use the official browser or device flow. If an earlier attempt stopped, use the visible retry/sign-in action.
- **Preparation failed:** use Repair in More options. Your Tender files and original documents are preserved.
- **API key rejected or model unavailable:** correct the key, endpoint or exact model name and retry.
- **Provider limit reached:** review that provider's allowance and retry when available. Quantix does not silently switch accounts or payment methods.
- **Old local service:** reopen Quantix through its launcher to load the current connection support.

Errors include a reference for the sanitized logs in `~/.quantix/logs`. **Settings → Technical details** shows recording status and the exact directory.

See [provider support and rules](ai-provider-support.md) and [the executed acceptance record](progress.md). Local diagnostics, real client metadata and live model checks are reported separately.
