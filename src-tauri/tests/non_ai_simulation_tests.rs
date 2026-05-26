use omnilauncher_lib::create_plugin_manager;

struct TypingCase {
    query: &'static str,
    expect_non_empty: bool,
    note: &'static str,
}

#[tokio::test]
async fn test_non_ai_typing_simulation_prefix_plugins() {
    let pm = create_plugin_manager();

    // Cases marked expect_non_empty=true are deterministic in normal dev environments.
    // Cases marked false are environment-dependent (bookmarks, clipboard contents, etc.)
    // but are still exercised to ensure no panics/regressions while typing.
    let cases = vec![
        TypingCase {
            query: "= 2+2",
            expect_non_empty: true,
            note: "calculator",
        },
        TypingCase {
            query: "git st",
            expect_non_empty: true,
            note: "git suggestions",
        },
        TypingCase {
            query: "net ports",
            expect_non_empty: true,
            note: "network",
        },
        TypingCase {
            query: "ps ",
            expect_non_empty: true,
            note: "process manager",
        },
        TypingCase {
            query: "env PATH",
            expect_non_empty: true,
            note: "env vars",
        },
        TypingCase {
            query: "color red",
            expect_non_empty: true,
            note: "color picker",
        },
        TypingCase {
            query: "sys lock",
            expect_non_empty: true,
            note: "system commands",
        },
        TypingCase {
            query: "timer 5s",
            expect_non_empty: true,
            note: "timer",
        },
        TypingCase {
            query: "conv 1 km to m",
            expect_non_empty: true,
            note: "unit converter",
        },
        TypingCase {
            query: "hosts localhost",
            expect_non_empty: true,
            note: "hosts",
        },
        TypingCase {
            query: "snip ",
            expect_non_empty: true,
            note: "snippets empty/help",
        },
        TypingCase {
            query: "todo list",
            expect_non_empty: true,
            note: "todo list/help",
        },
        TypingCase {
            query: "cron */5 * * * *",
            expect_non_empty: true,
            note: "cron explainer",
        },
        TypingCase {
            query: "emoji fire",
            expect_non_empty: true,
            note: "emoji picker",
        },
        TypingCase {
            query: "pomo",
            expect_non_empty: true,
            note: "pomodoro",
        },
        TypingCase {
            query: "sched",
            expect_non_empty: true,
            note: "scheduler",
        },
        TypingCase {
            query: "resize left",
            expect_non_empty: true,
            note: "window resize",
        },
        TypingCase {
            query: "bm example",
            expect_non_empty: false,
            note: "bookmarks depend on local browser profiles",
        },
        TypingCase {
            query: "cb secret",
            expect_non_empty: false,
            note: "clipboard history is environment-dependent",
        },
    ];

    for case in cases {
        let mut typed = String::new();

        // Mimic UI typing one character at a time.
        for ch in case.query.chars() {
            typed.push(ch);
            let _ = pm.query_all(&typed).await;
        }

        let final_results = pm.query_all(case.query).await;

        if case.expect_non_empty {
            assert!(
                !final_results.is_empty(),
                "Expected non-empty results for '{}' ({})",
                case.query,
                case.note
            );
        }
    }
}

#[tokio::test]
async fn test_non_ai_typing_simulation_prefix_conflicts() {
    let pm = create_plugin_manager();

    // Inputs that historically collide with prefix plugins should not route
    // to those plugins unless the prefix token is explicit.
    let non_prefix_inputs = [
        ("pomodoroapp", "pomo:"),
        ("scheduler", "sched:"),
        ("gitlab", "git:"),
    ];

    for (query, forbidden_prefix) in non_prefix_inputs {
        let mut typed = String::new();
        for ch in query.chars() {
            typed.push(ch);
            let _ = pm.query_all(&typed).await;
        }

        let final_results = pm.query_all(query).await;
        assert!(
            final_results
                .iter()
                .all(|r| !r.id.starts_with(forbidden_prefix)),
            "Query '{}' unexpectedly matched plugin id prefix '{}' with results: {:?}",
            query,
            forbidden_prefix,
            final_results.iter().map(|r| &r.id).collect::<Vec<_>>()
        );
    }

    // Exact prefix token forms should continue to match.
    let positive_cases = [
        ("pomo", "pomo:"),
        ("sched", "sched:"),
        ("git status", "git:"),
    ];

    for (query, expected_prefix) in positive_cases {
        let results = pm.query_all(query).await;
        assert!(
            results.iter().any(|r| r.id.starts_with(expected_prefix)),
            "Query '{}' failed to match expected plugin id prefix '{}'",
            query,
            expected_prefix
        );
    }
}
