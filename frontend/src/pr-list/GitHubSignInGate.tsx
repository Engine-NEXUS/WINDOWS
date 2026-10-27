import { useState } from "react";

export interface GitHubSignInGateProps {
  onSignIn?: (patToken?: string) => Promise<void> | void;
  isAuthenticating?: boolean;
  error?: string | null;
}

/**
 * Pixel-perfect GitHub Sign-In Gate Screen
 *
 * Implements the exact mobile twin authentication interface:
 * - Light Mode: Clean white canvas, black circular badge with white Octocat,
 *   solid black pill button with white "Sign in" typography, and subtle terms at bottom.
 * - Dark Mode: Deep black canvas, white circular badge with black Octocat,
 *   solid white pill button with black "Sign in" typography, and subtle terms at bottom.
 */
export function GitHubSignInGate({
  onSignIn = () => {},
  isAuthenticating = false,
  error = null,
}: GitHubSignInGateProps) {
  const [showPat, setShowPat] = useState(false);
  const [patInput, setPatInput] = useState("");

  const handlePatSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (patInput.trim()) {
      onSignIn(patInput.trim());
    }
  };

  return (
    <div className="gh-signin-gate" data-tauri-drag-region>
      {/* ── Top Spacer (flex: 1) to perfectly center the core cluster ── */}
      <div className="gh-signin-spacer" data-tauri-drag-region />

      {/* ── Center Identity & Action Cluster ── */}
      <div className="gh-signin-center">
        {/* GitHub Inverted Circular Emblem */}
        <div className="gh-signin-badge" aria-label="GitHub Logo">
          <svg
            className="gh-signin-octocat"
            viewBox="0 0 16 16"
            width="50"
            height="50"
            fill="currentColor"
            aria-hidden="true"
          >
            <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
          </svg>
        </div>

        {!showPat ? (
          <>
            {/* Primary Pill Sign-in Button */}
            <button
              type="button"
              className="gh-signin-btn"
              onClick={() => !isAuthenticating && onSignIn()}
              disabled={isAuthenticating}
              aria-label="Sign in to GitHub"
            >
              {isAuthenticating ? (
                <span className="gh-signin-btn-loading">
                  <span className="gh-signin-spinner" aria-hidden="true" />
                  <span>Connecting to GitHub...</span>
                </span>
              ) : (
                <span>Sign in</span>
              )}
            </button>

            <button
              type="button"
              className="gh-pat-toggle-btn"
              onClick={() => setShowPat(true)}
            >
              or connect with access token
            </button>
          </>
        ) : (
          <form className="gh-pat-form" onSubmit={handlePatSubmit}>
            <input
              type="password"
              className="gh-pat-input"
              placeholder="ghp_... or gho_... token"
              value={patInput}
              onChange={(e) => setPatInput(e.target.value)}
              autoFocus
            />
            <div className="gh-pat-actions">
              <button
                type="submit"
                className="gh-signin-btn gh-pat-submit-btn"
                disabled={isAuthenticating || !patInput.trim()}
              >
                {isAuthenticating ? "Connecting..." : "Save & Connect"}
              </button>
              <button
                type="button"
                className="gh-pat-back-btn"
                onClick={() => setShowPat(false)}
              >
                Cancel
              </button>
            </div>
          </form>
        )}

        {/* Error notification if connection encounters an issue */}
        {error && <div className="gh-signin-error">{error}</div>}
      </div>

      {/* ── Bottom Legal / Policy Terms ── */}
      <footer className="gh-signin-terms" data-tauri-drag-region>
        <p className="gh-signin-terms-text">
          By signing in, you accept our{" "}
          <a
            href="https://docs.github.com/site-policy/github-terms/github-terms-of-service"
            target="_blank"
            rel="noreferrer"
            className="gh-signin-link"
          >
            Terms of Use
          </a>{" "}
          and{" "}
          <a
            href="https://docs.github.com/site-policy/privacy-policies/github-privacy-statement"
            target="_blank"
            rel="noreferrer"
            className="gh-signin-link"
          >
            Privacy Policy
          </a>
        </p>
      </footer>
    </div>
  );
}
