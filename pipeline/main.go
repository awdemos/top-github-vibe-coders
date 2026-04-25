package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"dagger.io/dagger"
)

// Nomination represents a nomination file
type Nomination struct {
	GithubLogin  string `toml:"github_login" json:"github_login"`
	EvidenceURL  string `toml:"evidence_url,omitempty" json:"evidence_url,omitempty"`
	Nominator    struct {
		GithubLogin string `toml:"github_login" json:"github_login"`
		Reason      string `toml:"reason,omitempty" json:"reason,omitempty"`
	} `toml:"nominator" json:"nominator"`
}

// PipelineReport is the structured output of the eligibility pipeline
type PipelineReport struct {
	GithubLogin      string          `json:"github_login"`
	Verdict          string          `json:"verdict"` // approved | rejected
	VibeScore        float64         `json:"vibe_score"`
	ScoreBreakdown   ScoreBreakdown  `json:"score_breakdown"`
	Stats            UserStats       `json:"stats"`
	DesloppifyScores []RepoScore     `json:"desloppify_scores,omitempty"`
	Reason           string          `json:"reason"`
	Timestamp        time.Time       `json:"timestamp"`
}

type ScoreBreakdown struct {
	CommitVelocity   int `json:"commit_velocity"`
	LOCExplosion     int `json:"loc_explosion"`
	AIPatterns       int `json:"ai_patterns"`
	RepoProliferation int `json:"repo_proliferation"`
	DesloppifySlop   int `json:"desloppify_slop"`
}

type UserStats struct {
	Commits90d   int `json:"commits_90d"`
	LOC90d       int `json:"loc_90d"`
	RepoCount90d int `json:"repo_count_90d"`
}

type RepoScore struct {
	RepoName string  `json:"repo_name"`
	Score    float64 `json:"score"`
}

func main() {
	ctx := context.Background()

	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "Usage: go run main.go <nomination-file.toml>")
		os.Exit(1)
	}

	nominationFile := os.Args[1]
	githubToken := os.Getenv("GITHUB_TOKEN")
	if githubToken == "" {
		fmt.Fprintln(os.Stderr, "GITHUB_TOKEN is required")
		os.Exit(1)
	}

	if err := run(ctx, nominationFile, githubToken); err != nil {
		fmt.Fprintf(os.Stderr, "Pipeline failed: %v\n", err)
		os.Exit(1)
	}
}

func run(ctx context.Context, nominationFile, githubToken string) error {
	client, err := dagger.Connect(ctx, dagger.WithLogOutput(os.Stderr))
	if err != nil {
		return err
	}
	defer client.Close()

	// Read nomination file
	nominationData, err := os.ReadFile(nominationFile)
	if err != nil {
		return fmt.Errorf("read nomination file: %w", err)
	}

	var nomination Nomination
	// Simple TOML parsing by hand for the fields we care about
	inNomineeSection := false
	for _, line := range strings.Split(string(nominationData), "\n") {
		line = strings.TrimSpace(line)
		if strings.HasPrefix(line, "[[nominee]]") || strings.HasPrefix(line, "[nominee]") {
			inNomineeSection = true
			continue
		}
		if strings.HasPrefix(line, "[") && !strings.HasPrefix(line, "[nominee") {
			inNomineeSection = false
			continue
		}
		if inNomineeSection && strings.HasPrefix(line, "github_login") {
			parts := strings.SplitN(line, "=", 2)
			if len(parts) == 2 {
				nomination.GithubLogin = strings.Trim(strings.TrimSpace(parts[1]), `"`)
				break
			}
		}
	}

	if nomination.GithubLogin == "" {
		return fmt.Errorf("could not parse github_login from nomination file")
	}

	fmt.Fprintf(os.Stderr, "Evaluating nomination for: %s\n", nomination.GithubLogin)

	// Build a container with curl, jq, git, and desloppify
	pipeline := client.Container().
		From("python:3.12-slim").
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "git", "curl", "jq"}).
		WithExec([]string{"pip", "install", "desloppify"}).
		WithSecretVariable("GITHUB_TOKEN", client.SetSecret("GITHUB_TOKEN", githubToken)).
		WithEnvVariable("NOMINEE_LOGIN", nomination.GithubLogin)

	// Fetch user events from GitHub API
	eventsScript := `
#!/bin/bash
set -e
USER="$NOMINEE_LOGIN"
TOKEN="$GITHUB_TOKEN"

# Get public events
curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  -H "User-Agent: vibe-pipeline" \
  "https://api.github.com/users/$USER/events/public?per_page=100" > /tmp/events.json

# Count PushEvents in last 90 days
NINETY_DAYS_AGO=$(date -d '90 days ago' +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -v-90d +%Y-%m-%dT%H:%M:%SZ)

commits=$(cat /tmp/events.json | jq -r '[.[] | select(.type == "PushEvent") | .payload.commits | length] | add // 0')
echo "COMMITS=$commits"

# Count repos created in last 90 days
repos=$(curl -s -H "Authorization: Bearer $TOKEN" \
  -H "Accept: application/vnd.github.v3+json" \
  "https://api.github.com/users/$USER/repos?sort=created&per_page=100" | \
  jq -r --arg since "$NINETY_DAYS_AGO" '[.[] | select(.created_at >= $since)] | length')
echo "REPOS=$repos"

# Get top 3 repos by push activity
top_repos=$(cat /tmp/events.json | jq -r '[.[] | select(.type == "PushEvent") | .repo.name] | group_by(.) | map({name: .[0], count: length}) | sort_by(.count) | reverse | .[0:3] | .[].name')
echo "TOP_REPOS=$top_repos"

# Detect AI patterns in commit messages
ai_patterns=$(cat /tmp/events.json | jq -r '[.[] | select(.type == "PushEvent") | .payload.commits[]? | select(.message | test("generated by|copilot|ai.?assist|vibe|claude|gpt|llm"; "i"))] | length')
echo "AI_PATTERNS=$ai_patterns"
`

	pipeline = pipeline.WithNewFile("/scripts/fetch_stats.sh", dagger.ContainerWithNewFileOpts{
		Contents: eventsScript,
	})
	statsOutput, err := pipeline.WithExec([]string{"bash", "/scripts/fetch_stats.sh"}).Stdout(ctx)
	if err != nil {
		return fmt.Errorf("fetch stats failed: %w", err)
	}

	// Parse stats output
	var commits, repos, aiPatterns int
	var topRepos []string
	for _, line := range strings.Split(statsOutput, "\n") {
		if strings.HasPrefix(line, "COMMITS=") {
			commits, _ = strconv.Atoi(strings.TrimPrefix(line, "COMMITS="))
		} else if strings.HasPrefix(line, "REPOS=") {
			repos, _ = strconv.Atoi(strings.TrimPrefix(line, "REPOS="))
		} else if strings.HasPrefix(line, "AI_PATTERNS=") {
			aiPatterns, _ = strconv.Atoi(strings.TrimPrefix(line, "AI_PATTERNS="))
		} else if strings.HasPrefix(line, "TOP_REPOS=") {
			repoName := strings.TrimPrefix(line, "TOP_REPOS=")
			if repoName != "" {
				topRepos = append(topRepos, repoName)
			}
		}
	}

	fmt.Fprintf(os.Stderr, "Stats: commits=%d repos=%d ai_patterns=%d\n", commits, repos, aiPatterns)

	// Clone top repos and run desloppify
	var desloppifyScores []RepoScore
	var totalDesloppify float64

	for _, repoName := range topRepos {
		if strings.Contains(repoName, "/") {
			parts := strings.Split(repoName, "/")
			if len(parts) == 2 {
				owner, repo := parts[0], parts[1]
				repoDir := fmt.Sprintf("/repos/%s", repo)

				cloneScript := fmt.Sprintf(`
#!/bin/bash
set -e
OWNER=%q
REPO=%q
REPO_DIR=%q
git clone --depth 1 "https://github.com/$OWNER/$REPO.git" "$REPO_DIR" 2>/dev/null || true
if [ -d "$REPO_DIR" ]; then
  cd "$REPO_DIR"
  desloppify scan --path . --format json 2>/dev/null | jq '.overall_score // 50' || echo "50"
else
  echo "50"
fi
`, owner, repo, repoDir)

				scoreOutput, err := pipeline.WithExec([]string{"bash", "-c", cloneScript}).Stdout(ctx)
				if err != nil {
					fmt.Fprintf(os.Stderr, "Desloppify failed for %s: %v\n", repoName, err)
					continue
				}

				scoreStr := strings.TrimSpace(scoreOutput)
				score, _ := strconv.ParseFloat(scoreStr, 64)
				desloppifyScores = append(desloppifyScores, RepoScore{
					RepoName: repoName,
					Score:    score,
				})
				totalDesloppify += score
			}
		}
	}

	var avgDesloppify float64
	if len(desloppifyScores) > 0 {
		avgDesloppify = totalDesloppify / float64(len(desloppifyScores))
	} else {
		avgDesloppify = 50 // neutral default
	}

	fmt.Fprintf(os.Stderr, "Avg desloppify score: %.1f\n", avgDesloppify)

	// Calculate Vibe Score (0-100)
	// Commit Velocity (0-25): >500=25, >200=15, >100=5
	commitScore := 0
	if commits >= 500 {
		commitScore = 25
	} else if commits >= 200 {
		commitScore = 15
	} else if commits >= 100 {
		commitScore = 5
	}

	// LOC Explosion (0-20): estimate 150 LOC per commit
	estLOC := commits * 150
	locScore := 0
	if estLOC >= 50000 {
		locScore = 20
	} else if estLOC >= 20000 {
		locScore = 12
	} else if estLOC >= 5000 {
		locScore = 5
	}

	// AI Patterns (0-20): >20=20, >10=12, >5=5
	aiScore := 0
	if aiPatterns >= 20 {
		aiScore = 20
	} else if aiPatterns >= 10 {
		aiScore = 12
	} else if aiPatterns >= 5 {
		aiScore = 5
	}

	// Repo Proliferation (0-15): >10=15, >5=8, >3=3
	repoScore := 0
	if repos >= 10 {
		repoScore = 15
	} else if repos >= 5 {
		repoScore = 8
	} else if repos >= 3 {
		repoScore = 3
	}

	// Desloppify Slop (0-20): inverted - lower score = higher vibe
	// <30 = 20, <50 = 12, <70 = 5
	deslopScore := 0
	if avgDesloppify < 30 {
		deslopScore = 20
	} else if avgDesloppify < 50 {
		deslopScore = 12
	} else if avgDesloppify < 70 {
		deslopScore = 5
	}

	totalScore := commitScore + locScore + aiScore + repoScore + deslopScore

	// Verdict: >=60 with at least 3 categories >10
	categoriesAbove10 := 0
	for _, s := range []int{commitScore, locScore, aiScore, repoScore, deslopScore} {
		if s >= 10 {
			categoriesAbove10++
		}
	}

	verdict := "rejected"
	reason := "Does not meet vibe coder criteria"
	if totalScore >= 60 && categoriesAbove10 >= 3 {
		verdict = "approved"
		reason = fmt.Sprintf("Strong vibe coder signals: %d commits, %d new repos, %d AI patterns, desloppify=%.1f",
			commits, repos, aiPatterns, avgDesloppify)
	} else if totalScore < 60 {
		reason = fmt.Sprintf("Vibe score too low (%d/100). Need 60+ with 3+ strong categories.", totalScore)
	} else {
		reason = fmt.Sprintf("Only %d/5 strong categories. Need 3+.", categoriesAbove10)
	}

	report := PipelineReport{
		GithubLogin: nomination.GithubLogin,
		Verdict:     verdict,
		VibeScore:   float64(totalScore),
		ScoreBreakdown: ScoreBreakdown{
			CommitVelocity:    commitScore,
			LOCExplosion:      locScore,
			AIPatterns:        aiScore,
			RepoProliferation: repoScore,
			DesloppifySlop:    deslopScore,
		},
		Stats: UserStats{
			Commits90d:   commits,
			LOC90d:       estLOC,
			RepoCount90d: repos,
		},
		DesloppifyScores: desloppifyScores,
		Reason:           reason,
		Timestamp:        time.Now(),
	}

	reportJSON, err := json.MarshalIndent(report, "", "  ")
	if err != nil {
		return err
	}

	fmt.Println(string(reportJSON))

	// Write report to file for CI to pick up
	reportPath := filepath.Join(filepath.Dir(nominationFile), 
		nomination.GithubLogin+"_report.json")
	if err := os.WriteFile(reportPath, reportJSON, 0644); err != nil {
		return fmt.Errorf("write report: %w", err)
	}

	if verdict == "rejected" {
		return fmt.Errorf("nomination rejected: %s", reason)
	}

	return nil
}
