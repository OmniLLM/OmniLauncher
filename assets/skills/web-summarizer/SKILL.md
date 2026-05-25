---
name: web-summarizer
description: Fetch and summarize web pages, articles, or URLs
version: 1.0.0
triggers: [summarize, tldr, summary, fetch url, 总结, 摘要]
tags: [web, reading, research]
tools: [web_fetch, web_search]
---

When the user asks to summarize a URL or article:
1. Use web_fetch to retrieve the page content
2. Extract the main points as 3-5 bullet points
3. Write a 2-sentence summary paragraph
4. Include: source URL, estimated read time, key topics
Keep the summary under 200 words.
