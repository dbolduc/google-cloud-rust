// Copyright 2024 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package sidekick

import (
	"fmt"
	"os"
	"regexp"
	"strings"

	"github.com/cbroglie/mustache"
	"github.com/googleapis/google-cloud-rust/generator/internal/config"
)

func init() {
	newCommand(
		"sidekick rust-refreshall",
		"Reruns the generator for all client libraries.",
		`
Reruns the generator for all client libraries, using the configuration parameters saved in the .sidekick.toml file for each library.

Applies Rust-specific edits after generation is complete. Specifically, we use this command to write the GCS admin surface.
`,
		cmdSidekick,
		rustRefreshAll,
	).
		addAltName("rust-refreshall").
		addAltName("rustRefreshAll")
}

func rustRefreshAll(rootConfig *config.Config, cmdLine *CommandLine) error {
	// TODO : uncomment, this is just useless when iterating.
	//if err := refreshAll(rootConfig, cmdLine); err != nil {
	//      	return fmt.Errorf("got an error trying to refresh libraries: %w", err)

	//}
	storageClient, err := readClient("storage", "src/storage/src/generated/gapic/client.rs")
	if err != nil {
		return err
	}
	controlClient, err := readClient("control", "src/storage/src/generated/gapic_control/client.rs")
	if err != nil {
		return err
	}
	functions := fmt.Sprintf("%s\n\n%s", storageClient, controlClient)

	client_rs, err := mustache.RenderFile("generator/internal/sidekick/storage_control_client.mustache", map[string]string{"Functions": functions})
	if err != nil {
		return err
	}

	// TODO: this ought to live in a generated/ dir. Hm.
	os.WriteFile("src/storage/src/control/client.rs", []byte(client_rs), 0666)
	if err != nil {
		return err
	}
	return nil
}

// TODO : rename to like: extract public methods
func readClient(client string, path string) (string, error) {
	contentBytes, err := os.ReadFile(path)
	if err != nil {
		return "", fmt.Errorf("error reading %s: %w", path, err)
	}

	contents := string(contentBytes)
	paragraphs := strings.Split(contents, "\n\n")

	var processed []string
	for _, p := range paragraphs {
		if !strings.HasPrefix(p, " ") || !strings.Contains(p, "pub fn ") {
			// Skip non-indented paragraphs and non-public member functions.
			continue
		}
		p, err := rewrite(client, p)
		if err != nil {
			return "", err
		}
		processed = append(processed, p)
	}
	return strings.Join(processed, "\n\n"), nil
}

// Rewrites the implementation of a method
func rewrite(client string, paragraph string) (string, error) {
	re := regexp.MustCompile(`(?m)^\s*pub fn (\w+)\(`)

	indices := re.FindStringSubmatchIndex(paragraph)
	if indices == nil {
		return "", fmt.Errorf("could not find a valid function definition")
	}

	// The second capture group matches the method name. This corresponds
	// to indices 2 and 3.
	methodStart := indices[2]
	methodEnd := indices[3]
	method := paragraph[methodStart:methodEnd]

	// Look for the opening `{`. We start from the end of the method name to
	// make sure we skip past the comments.
	bodyIndexRelative := strings.Index(paragraph[methodEnd:], "{")
	if bodyIndexRelative == -1 {
		return "", fmt.Errorf("could not find opening brace '{' after function definition")
	}
	bodyIndex := methodEnd + bodyIndexRelative

	// Fix the builder name
	newReturnType := strings.ReplaceAll(paragraph[methodEnd:bodyIndex], "super::builder", "crate::builder")

	// Rewrite the body
	newBody := fmt.Sprintf(`{
        self.%s.%s()
    }`, client, method)

	return paragraph[:methodEnd] + newReturnType + newBody, nil
}
