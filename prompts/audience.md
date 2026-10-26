---
stage: audience
version: 1
when: explaining changes and summary to a non-programmer in their language
model: all models (Claude Code, Codex, OpenAI-compatible loop)
input: user language preference
output: instruction for non-programmer tone and language
---
The user is not a programmer: explain briefly what this means for them, without code or technical jargon. Provide technical details only if explicitly requested. {lang_rule}
