# FindOut privacy policy

Last updated: 22 September 2026

This policy covers the FindOut desktop app, progressive web app (PWA), backend,
and feedback form. FindOut answers questions by sending them to external model
and search services. Do not submit information that you do not want those
services to process.

## What a question sends

For an ordinary question, FindOut sends the backend:

- the question;
- up to four recent question and answer pairs when they are needed for a
  follow-up;
- a short device context such as operating system and package manager when the
  desktop app supplies it; and
- one attached image, if the user chooses one.

The backend sends that material to the configured model service. Production
currently uses the DeepSeek API first and may use OpenRouter as a bounded
fallback. If the model requests web search, the generated search query is sent
to Exa and the returned excerpts are sent back to the model. A currency
conversion sends only the amount and currency codes to Frankfurter.

FindOut does not use questions, answers, images, or search text for advertising.
The application backend does not intentionally save that content after the
request finishes and does not put it in application logs. Model, search, hosting,
and network providers still process the data in transit and apply their own
retention rules.

Current provider information:

- [DeepSeek API documentation and terms](https://api-docs.deepseek.com/api/deepseek-api/)
  and [context-cache retention](https://api-docs.deepseek.com/guides/kv_cache/)
- [OpenRouter privacy policy](https://openrouter.ai/privacy/)
- [Exa privacy policy](https://exa.ai/privacy-policy)
- [Vercel privacy notice](https://vercel.com/legal/privacy-notice)
- [Supabase privacy policy](https://supabase.com/privacy)
- [Frankfurter](https://frankfurter.dev/)

FindOut does not currently use a commerce provider or collect payment details.
This policy will be updated before payment processing is introduced.

DeepSeek documents automatic prompt-context caching that normally clears within
hours to days. OpenRouter may route a fallback request to its selected model
provider, whose practices can differ. Exa states that query data may be used to
improve its products and technology. These provider terms can change; the links
above are the controlling sources.

## Activation, usage, and hosting data

The PWA stores its installation token in a Secure, HttpOnly, SameSite cookie.
The desktop app stores it in the operating system keychain. Tokens expire after
one year and can be removed earlier with **Forget activation** in the PWA or by
uninstalling the desktop app. A pseudonymous trial device value stays on the
device so the same trial can be restored. Raw machine identifiers are not sent
or stored by FindOut.

The backend stores a shortened hash-derived license ID, license state,
per-license UTC daily request counts, and shared provider-health state in
Supabase. Deployments with the selected cost-quota release also store
short-window rate state and aggregate daily service usage. Aggregate usage
includes new and limited trial enrollments, admitted and limited requests,
estimated cost, searches, images, fallbacks, failures, and incomplete requests.
That release also creates opaque per-query reservation rows containing the
shortened license ID and operational counts, but no user content;
they become eligible for cleanup at 48 hours and are removed by the next hourly
run, normally within 49 hours. Per-license daily counters keep the current and
previous UTC day, and aggregate daily usage is kept for 35 days. There is no
automatic expiry for an active license record or its rate state; users can
request deletion as described below.

Supabase does not receive the activation key, installation token, raw device
value, question, answer, screenshot, search text, or conversation history from
the quota system. These service records exist to provide activation, enforce and
tune free-service limits, investigate abuse, or meet legal obligations.

Vercel hosts the PWA and backend and therefore processes network information such
as IP address, route, time, status, and user agent. FindOut application logs add
only operational fields such as timing, model route, fallback reason, provider
attempt status, and tool counts. They omit license and reservation IDs, raw
questions, answers, images, credentials, installation tokens, and model
reasoning. FindOut does not create a separate content log. Vercel's standard
runtime-log window varies by plan from one hour to 30 days; its privacy notice
may allow longer processing for security and legal needs.

## Data kept on the device

The clients keep at most five recent conversation threads on the device. The PWA
uses browser local storage. The desktop app uses a user-only local file. Attached
image bytes and installation tokens are not written into conversation history.
History remains until the user selects **Clear recent history**, clears the
browser's site data, or uninstalls the desktop app. Clipboard copies and locally
saved feedback files are controlled by the user and remain until the user deletes
them.

FindOut does not add advertising cookies or third-party analytics. The PWA uses
the activation cookie described above and a service-worker cache for its public
application files.

## Feedback and support

Before each submission, the feedback form lists the complete payload and requires
confirmation. A submission contains only:

- the text typed into the feedback field;
- the reply email typed by the user, if any;
- the displayed app version; and
- the pseudonymous license and installation IDs only when the user selects that
  option.

The form never automatically attaches a conversation, answer, image, clipboard
contents, activation key, installation token, or diagnostics. Anything pasted
into the feedback text is part of the submitted message. Feedback works without
activation and without sharing support IDs.

The backend sends the displayed payload through AgentMail to the support address.
It does not send it to a model and does not save a second copy. FindOut's support
policy is to delete a report from the support mailbox within 90 days of receipt.
If a conversation is still active, it may be retained until 30 days after the
case closes, up to 180 days total, unless a longer period is needed for a legal or
security matter. AgentMail says deleted mailbox data may remain in point-in-time
recovery for up to 35 days. See the
[AgentMail privacy policy](https://www.agentmail.to/legal/privacy).

If delivery fails, the clients keep the draft in the open form. The PWA can save
it as a local text file and the desktop app can copy it to the clipboard. Neither
fallback sends data anywhere.

## Choices and deletion requests

Users can:

- clear local conversation history at any time;
- remove the PWA activation cookie with **Forget activation** or remove desktop
  credentials and local data by uninstalling;
- leave the optional reply email blank;
- decline to share license and installation IDs; and
- close the feedback form or keep a local copy instead of sending it.

To ask about this policy or request deletion of a support report, license record,
or associated aggregate service records, email **moeg-5@agentmail.to**. Do not
send an activation key or installation token. Include the reply email used for a
support report or opt in to sharing the pseudonymous support IDs so the record can
be located. Some minimal information may be retained when required to address
security abuse or a legal obligation.

Material changes will update the date at the top of this policy.
