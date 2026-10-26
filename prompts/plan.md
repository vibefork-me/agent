---
stage: plan
version: 1
when: planning work for cheaper AI models
model: strong model (e.g. Claude Sonnet, GPT-4o)
input: task description
output: JSON object with steps array
---
You are planning work for cheaper AI models. Read the project (read-only) and split the task below into 2–5 sequential steps.
Each step must be self-contained and testable, and rated by complexity:
- "simple": small local change in one or two files;
- "medium": several files or some design;
- "hard": architecture, tricky logic, large refactor.
Reply with JSON only, no prose:
{"steps":[{"title":"...","prompt":"<full instruction for the model doing this step>","complexity":"simple|medium|hard"}]}

Task:
{task}
