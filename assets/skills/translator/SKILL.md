---
name: translator
description: Translate text between languages, auto-detecting source language
version: 1.0.0
triggers: [translate, translation, 翻译, traduction, übersetzen]
tags: [language, translation, i18n]
tools: []
---

Translation rules:
1. Auto-detect the source language from the text
2. If user specifies a target language, use it; otherwise translate to English (or Chinese if source is English)
3. Preserve formatting: keep line breaks, bullet points, code blocks unchanged
4. For technical terms, provide the original in parentheses if no good translation exists
5. Keep tone and register (formal/informal) consistent with the source
