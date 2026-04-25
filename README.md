# 🏆 Top GitHub Vibe Coders

A leaderboard for developers pushing massive amounts of AI-assisted code on GitHub.

## What is a Vibe Coder?

A "vibe coder" builds software by describing what they want in plain English and letting AI write most of the code — often without reading every line. This project celebrates (and tracks) the most prolific ones.

## Features

- **Leaderboard**: Ranked list of validated vibe coders with commit stats and vibe scores
- **Community Voting**: GitHub-authenticated users vote for their favorites
- **Claw Bot Hall of Fame**: Separate voting track for autonomous AI agent accounts
- **Dagger Pipeline**: PR-triggered eligibility engine that calculates Vibe Scores
- **Desloppify Integration**: Code quality analysis to detect AI-generated "slop"

## Tech Stack

- **Backend**: Rust + Axum
- **Frontend**: Askama SSR + HTMX
- **Database**: Turso (libsql)
- **Pipeline**: Dagger (Go SDK)
- **Deployment**: Docker

## Getting Started

```bash
# Copy env template
cp .env.example .env
# Fill in your GitHub OAuth app credentials and Turso token

# Run with Docker
docker compose up --build

# Or locally
cargo run
```

## Nomination Flow

1. Fork this repo
2. Copy `nominations/TEMPLATE.toml` to `nominations/{github_username}.toml`
3. Fill in the target user's details
4. Open a PR
5. The Dagger pipeline automatically evaluates eligibility and posts results

## Vibe Score Calculation

The pipeline calculates a score (0-100) based on:

- **Commit Velocity** (0-25): Commits in last 90 days
- **LOC Explosion** (0-20): Estimated lines of code added
- **AI Patterns** (0-20): AI-indicator commit messages
- **Repo Proliferation** (0-15): New repos created
- **Desloppify Slop** (0-20): Inverted code health score

Threshold: **≥60** with **3+ strong categories** to qualify.

## License

MIT
