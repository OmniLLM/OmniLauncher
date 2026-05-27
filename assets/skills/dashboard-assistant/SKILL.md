---
name: dashboard-assistant
description: Navigate and open different system dashboard pages (Overview, Todos, AI Conversation, Scheduler, Database) in your default browser.
version: 1.0.0
triggers: [dashboard, todo, open todo, open discussion, database, tables, jobs, schedules, scheduler, overview]
tags: [dashboard, browser, workflow, navigation]
tools: [open_url]
---

Guide the user to the correct dashboard pages based on their request. Use the `open_url` tool to open these pages in their default browser:

Available Dashboard Routes:
1. **Overview / Dashboard Home**: `http://localhost:3000/dashboard` or `/dashboard` (displays general metrics and links).
2. **Todos**: `/dashboard/todos` (displays interactive todo lists, progress meters, tasks list).
3. **AI Conversation / Chat History**: `/dashboard/conversation` (shows prior chats, assistants logs).
4. **Scheduler (Jobs)**: `/dashboard/jobs` (shows status of background scheduled tasks and jobs).
5. **Database (Tables)**: `/dashboard/tables` (browse raw database schemas and SQLite table contents).

Rules:
1. If the user asks to see/open their todos, call `open_url` with the correct route (e.g. `/dashboard/todos` or its absolute localhost URL).
2. If the user asks about background tasks, cron, scheduler, or jobs, call `open_url` with `/dashboard/jobs`.
3. If they ask about databases, sql, schemas, or tables, call `open_url` with `/dashboard/tables`.
4. If they ask about overall dashboard, statistics, index, or home, open `/dashboard`.
5. If they refer to chats, conversation, or AI talk, open `/dashboard/conversation`.
6. Inform the user in text which page you are opening.
