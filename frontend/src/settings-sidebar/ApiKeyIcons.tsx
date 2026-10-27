import React from "react";

export interface LogoProps extends React.SVGProps<SVGSVGElement> {
  size?: number;
}

/**
 * High-fidelity brand logos for all API key providers supported in NEXUS.
 * Every logo is an ultra-crisp, scalable vector graphic with official brand colors.
 */
export const ApiKeyIcons = {
  // Groq official brand orange (#F55036) with crisp geometric glyph
  Groq: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Groq"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#F55036" />
      {/* Groq iconic stylized lowercase 'g' mark with circular core */}
      <path
        d="M21 11H15C12.2386 11 10 13.2386 10 16C10 18.7614 12.2386 21 15 21H21.5V23.8C21.5 25.5673 20.0673 27 18.3 27H11.5"
        stroke="#FFFFFF"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="15.5" cy="16" r="2.3" fill="#FFFFFF" />
    </svg>
  ),

  // Google Gemini official 4-pointed radiant sparkle star with multi-stop AI gradient
  Gemini: ({ size = 28, className = "", ...props }: LogoProps) => {
    const id = React.useId().replace(/:/g, "");
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 32 32"
        fill="none"
        className={className}
        role="img"
        aria-label="Google Gemini"
        {...props}
      >
        <defs>
          <linearGradient id={`gemini-bg-${id}`} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#1B1A38" />
            <stop offset="100%" stopColor="#0B0D1B" />
          </linearGradient>
          <linearGradient id={`gemini-sparkle-${id}`} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#4E82EE" />
            <stop offset="35%" stopColor="#7F56D9" />
            <stop offset="70%" stopColor="#C15599" />
            <stop offset="100%" stopColor="#F59E0B" />
          </linearGradient>
        </defs>
        <rect
          width="32"
          height="32"
          rx="7"
          fill={`url(#gemini-bg-${id})`}
          stroke="rgba(127, 86, 217, 0.35)"
          strokeWidth="1"
        />
        {/* Exact Bezier 4-point Gemini sparkle */}
        <path
          d="M16 4C16 10.627 10.627 16 4 16C10.627 16 16 21.373 16 28C16 21.373 21.373 16 28 16C21.373 16 16 10.627 16 4Z"
          fill={`url(#gemini-sparkle-${id})`}
        />
      </svg>
    );
  },

  // Cerebras official neural circuit / wafer-scale hexagon in vivid ruby-crimson
  Cerebras: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Cerebras"
      {...props}
    >
      <rect
        width="32"
        height="32"
        rx="7"
        fill="#1C1117"
        stroke="rgba(255, 23, 81, 0.3)"
        strokeWidth="1"
      />
      {/* Hexagonal circuit wafer core */}
      <path
        d="M16 6L24.66 11V21L16 26L7.34 21V11L16 6Z"
        stroke="#FF1751"
        strokeWidth="2.2"
        strokeLinejoin="round"
      />
      <circle cx="16" cy="16" r="3.2" fill="#FF1751" />
      <line x1="16" y1="6" x2="16" y2="12.8" stroke="#FF1751" strokeWidth="1.8" />
      <line x1="16" y1="19.2" x2="16" y2="26" stroke="#FF1751" strokeWidth="1.8" />
      <line x1="7.34" y1="16" x2="12.8" y2="16" stroke="#FF1751" strokeWidth="1.8" />
      <line x1="19.2" y1="16" x2="24.66" y2="16" stroke="#FF1751" strokeWidth="1.8" />
    </svg>
  ),

  // Deepgram official audio waveform & stylized geometric D in electric mint-cyan
  Deepgram: ({ size = 28, className = "", ...props }: LogoProps) => {
    const id = React.useId().replace(/:/g, "");
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 32 32"
        fill="none"
        className={className}
        role="img"
        aria-label="Deepgram"
        {...props}
      >
        <defs>
          <linearGradient id={`deepgram-grad-${id}`} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#13EF93" />
            <stop offset="100%" stopColor="#00E5FF" />
          </linearGradient>
        </defs>
        <rect
          width="32"
          height="32"
          rx="7"
          fill="#0D1829"
          stroke="rgba(19, 239, 147, 0.3)"
          strokeWidth="1"
        />
        {/* Deepgram stylized D */}
        <path
          d="M9.5 8H16.2C20.6 8 23.5 11.2 23.5 16C23.5 20.8 20.6 24 16.2 24H9.5V8Z"
          fill={`url(#deepgram-grad-${id})`}
        />
        <path
          d="M13.5 12H16C18.2 12 19.5 13.6 19.5 16C19.5 18.4 18.2 20 16 20H13.5V12Z"
          fill="#0D1829"
        />
        <circle cx="16.2" cy="16" r="1.6" fill={`url(#deepgram-grad-${id})`} />
      </svg>
    );
  },

  // Google Maps & Routes official 4-color navigation pin
  GoogleMaps: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Google Maps"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#FFFFFF" stroke="rgba(0,0,0,0.08)" strokeWidth="1" />
      <g transform="translate(5, 3.5) scale(0.68)">
        {/* Red pin head */}
        <path
          d="M16 2C9.37 2 4 7.37 4 14C4 22.5 16 30 16 30C16 30 28 22.5 28 14C28 7.37 22.63 2 16 2Z"
          fill="#EA4335"
        />
        {/* Blue right fold */}
        <path d="M16 2C22.63 2 28 7.37 28 14C28 18.5 24.5 23.5 20 26.5L16 14V2Z" fill="#4285F4" opacity="0.9" />
        {/* Green bottom curve */}
        <path d="M16 30C16 30 20 26.5 20 26.5L16 23L12 26.5C12 26.5 16 30 16 30Z" fill="#34A853" />
        {/* Yellow fold */}
        <path d="M12 26.5L16 23V14L9 19L12 26.5Z" fill="#FBBC04" />
        {/* Center circular cutout */}
        <circle cx="16" cy="13.5" r="4.5" fill="#FFFFFF" />
        <circle cx="16" cy="13.5" r="2.5" fill="#4285F4" />
      </g>
    </svg>
  ),

  // Google Places official red local marker beacon with center POI star
  GooglePlaces: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Google Places"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#EA4335" />
      {/* Location beacon */}
      <path
        d="M16 5.5C11.58 5.5 8 9.08 8 13.5C8 19 16 26.5 16 26.5C16 26.5 24 19 24 13.5C24 9.08 20.42 5.5 16 5.5Z"
        fill="#FFFFFF"
      />
      {/* Center 5-point star */}
      <path
        d="M16 9.8L17.2 12.3H19.8L17.7 13.8L18.5 16.3L16 14.6L13.5 16.3L14.3 13.8L12.2 12.3H14.8L16 9.8Z"
        fill="#EA4335"
      />
    </svg>
  ),

  // OpenAI iconic emerald rosette spiral
  OpenAI: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="OpenAI"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#10A37F" />
      <g transform="translate(6, 6) scale(0.83)" stroke="#FFFFFF" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M19.4 14.8C18.9 11.8 16.2 9.5 13 9.5C12.5 9.5 12 9.6 11.5 9.8V8C11.5 6.3 10.2 5 8.5 5C7.2 5 6.1 5.8 5.6 7C4.1 7.6 3 9.2 3 11C3 11.5 3.1 12 3.3 12.5C2.5 13.4 2 14.6 2 16C2 18.8 4.2 21 7 21V19C7 17.3 8.3 16 10 16C10.5 16 11 16.1 11.5 16.3C12.3 15.4 13.5 14.8 14.8 14.8H19.4Z" fill="none" />
        <circle cx="12" cy="12" r="3" fill="#FFFFFF" />
      </g>
    </svg>
  ),

  // Anthropic Claude warm terracotta starburst asterisk
  Anthropic: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Anthropic Claude"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#D97757" />
      <g transform="translate(7, 7) scale(0.75)" fill="#FFFFFF">
        <path d="M12 0L14.2 8.5L22.5 5.5L16.5 12L24 15L15.5 17.5L18.5 25.5L12 19.5L9 27L6.8 18.5L0 20.5L5.5 14.5L0 9.5L8.5 8.5L5.5 0.5L12 6.5L12 0Z" />
      </g>
    </svg>
  ),

  // ElevenLabs iconic sound bars
  ElevenLabs: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="ElevenLabs"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#000000" stroke="rgba(255,255,255,0.15)" strokeWidth="1" />
      {/* Two iconic vertical bars */}
      <rect x="11" y="8" width="3.5" height="16" rx="1.75" fill="#FFFFFF" />
      <rect x="17.5" y="8" width="3.5" height="16" rx="1.75" fill="#FFFFFF" />
    </svg>
  ),

  // GitHub Octocat
  GitHub: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="GitHub"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#24292F" stroke="rgba(255,255,255,0.12)" strokeWidth="1" />
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M16 6C10.477 6 6 10.484 6 16.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0116 10.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0026 16.017C26 10.484 21.522 6 16 6z"
        fill="#FFFFFF"
      />
    </svg>
  ),

  // Telegram official paper plane emblem
  Telegram: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Telegram"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#229ED9" />
      <path
        d="M7.5 15.5L24 9L19.5 23L15 19L11.5 21L12 17.5L21.5 12.5L10.5 16.5L7.5 15.5Z"
        fill="#FFFFFF"
        stroke="#FFFFFF"
        strokeWidth="0.8"
        strokeLinejoin="round"
      />
    </svg>
  ),

  // DeepSeek official oceanic blue with stylized leaping dolphin/whale mark
  DeepSeek: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="DeepSeek"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#1C64F2" />
      <g transform="translate(6, 6) scale(0.83)" fill="#FFFFFF">
        <path d="M12.5 3C7.25 3 3 7.25 3 12.5C3 17.75 7.25 22 12.5 22C14.8 22 16.9 21.2 18.5 19.8L21.7 23L23 21.7L19.8 18.5C21.2 16.9 22 14.8 22 12.5C22 7.25 17.75 3 12.5 3ZM12.5 5C16.65 5 20 8.35 20 12.5C20 14.3 19.3 16 18.1 17.2L16.5 15.6C17.1 14.7 17.5 13.6 17.5 12.5C17.5 9.75 15.25 7.5 12.5 7.5C11.4 7.5 10.3 7.9 9.4 8.5L7.8 6.9C9.1 5.7 10.7 5 12.5 5Z" />
      </g>
    </svg>
  ),

  // xAI Grok official minimalist geometric glyph
  xAI: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="xAI Grok"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#000000" stroke="rgba(255,255,255,0.2)" strokeWidth="1" />
      <path
        d="M9 8L15.2 16.5L9 24H11.2L16.2 17.8L20.8 24H23L16.5 15.2L22.4 8H20.2L15.5 13.8L11.2 8H9Z"
        fill="#FFFFFF"
      />
    </svg>
  ),

  // Mistral AI official orange-to-red stair-step block mark
  Mistral: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Mistral AI"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#0D0D11" stroke="rgba(255,105,0,0.3)" strokeWidth="1" />
      <g transform="translate(6, 6) scale(0.625)" fill="#FF6900">
        <rect x="0" y="0" width="6.4" height="6.4" fill="#FF5000" />
        <rect x="25.6" y="0" width="6.4" height="6.4" fill="#FF5000" />
        <rect x="0" y="6.4" width="12.8" height="6.4" fill="#FF6900" />
        <rect x="19.2" y="6.4" width="12.8" height="6.4" fill="#FF6900" />
        <rect x="0" y="12.8" width="32" height="6.4" fill="#FF8200" />
        <rect x="0" y="19.2" width="6.4" height="6.4" fill="#FFA000" />
        <rect x="12.8" y="19.2" width="6.4" height="6.4" fill="#FFA000" />
        <rect x="25.6" y="19.2" width="6.4" height="6.4" fill="#FFA000" />
        <rect x="0" y="25.6" width="6.4" height="6.4" fill="#FFB700" />
        <rect x="25.6" y="25.6" width="6.4" height="6.4" fill="#FFB700" />
      </g>
    </svg>
  ),

  // OpenRouter official turquoise-cyan nexus connectivity logo
  OpenRouter: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="OpenRouter"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#0E121E" stroke="rgba(56,189,248,0.35)" strokeWidth="1" />
      <circle cx="16" cy="16" r="8" stroke="#38BDF8" strokeWidth="2.2" strokeDasharray="3 3" />
      <circle cx="16" cy="16" r="3.5" fill="#38BDF8" />
      <circle cx="16" cy="7" r="2" fill="#818CF8" />
      <circle cx="25" cy="16" r="2" fill="#818CF8" />
      <circle cx="16" cy="25" r="2" fill="#818CF8" />
      <circle cx="7" cy="16" r="2" fill="#818CF8" />
    </svg>
  ),

  // Perplexity official teal woven infinity asterism
  Perplexity: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Perplexity"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#13232C" stroke="rgba(34,197,94,0.3)" strokeWidth="1" />
      <g transform="translate(7, 7) scale(0.75)" stroke="#22C55E" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" fill="none">
        <path d="M12 2V22M2 12H22M5 5L19 19M19 5L5 19" />
      </g>
    </svg>
  ),

  // Cohere official warm coral organic logo
  Cohere: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Cohere"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#1A1817" stroke="rgba(217,92,62,0.35)" strokeWidth="1" />
      <circle cx="12" cy="14" r="5" fill="#D95C3E" />
      <circle cx="19" cy="19" r="6" fill="#39594C" />
      <circle cx="21" cy="12" r="3.5" fill="#D39872" />
    </svg>
  ),

  // Ollama official friendly minimalist geometric llama glyph
  Ollama: ({ size = 28, className = "", ...props }: LogoProps) => (
    <svg
      width={size}
      height={size}
      viewBox="0 0 32 32"
      fill="none"
      className={className}
      role="img"
      aria-label="Ollama"
      {...props}
    >
      <rect width="32" height="32" rx="7" fill="#FFFFFF" />
      <g transform="translate(6, 4) scale(0.65)" fill="#111827">
        <path d="M14 2C13 2 12 3 12 4V10L9 11C7.5 11.5 6 13 6 15V22C6 24 7 26 9 27V29C9 30 10 30 10 29V27H19V29C19 30 20 30 20 29V27C22 26 23 24 23 22V17C23 15 22 13 20 12L16 11V4C16 3 15 2 14 2ZM11 7C11.5 7 12 7.5 12 8C12 8.5 11.5 9 11 9C10.5 9 10 8.5 10 8C10 7.5 10.5 7 11 7ZM17 7C17.5 7 18 7.5 18 8C18 8.5 17.5 9 17 9C16.5 9 16 8.5 16 8C16 7.5 16.5 7 17 7Z" />
      </g>
    </svg>
  ),

  // Fallback high-tech cipher key badge
  GenericKey: ({ size = 28, className = "", ...props }: LogoProps) => {
    const id = React.useId().replace(/:/g, "");
    return (
      <svg
        width={size}
        height={size}
        viewBox="0 0 32 32"
        fill="none"
        className={className}
        role="img"
        aria-label="API Key"
        {...props}
      >
        <defs>
          <linearGradient id={`key-grad-${id}`} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#F59E0B" />
            <stop offset="100%" stopColor="#D97706" />
          </linearGradient>
        </defs>
        <rect
          width="32"
          height="32"
          rx="7"
          fill="#1E1E24"
          stroke="rgba(245, 158, 11, 0.3)"
          strokeWidth="1"
        />
        <circle cx="13" cy="19" r="4.5" stroke={`url(#key-grad-${id})`} strokeWidth="2.2" />
        <path
          d="M16.5 15.5L23.5 8.5M23.5 8.5L25.5 10.5M21.5 10.5L23 12"
          stroke={`url(#key-grad-${id})`}
          strokeWidth="2.2"
          strokeLinecap="round"
        />
      </svg>
    );
  },
};

/**
 * Normalizes service IDs and renders the matching official logo.
 */
export function ApiKeyIcon({
  service,
  size = 32,
  className = "",
}: {
  service: string;
  size?: number;
  className?: string;
}) {
  const norm = (service || "").toLowerCase().replace(/[-_ ]/g, "");

  if (norm.includes("groq")) {
    return <ApiKeyIcons.Groq size={size} className={className} />;
  }
  if (norm.includes("gemini")) {
    return <ApiKeyIcons.Gemini size={size} className={className} />;
  }
  if (norm.includes("openai") || norm.includes("chatgpt") || norm.includes("gpt")) {
    return <ApiKeyIcons.OpenAI size={size} className={className} />;
  }
  if (norm.includes("anthropic") || norm.includes("claude")) {
    return <ApiKeyIcons.Anthropic size={size} className={className} />;
  }
  if (norm.includes("deepseek")) {
    return <ApiKeyIcons.DeepSeek size={size} className={className} />;
  }
  if (norm.includes("xai") || norm.includes("grok") || norm.includes("twitter")) {
    return <ApiKeyIcons.xAI size={size} className={className} />;
  }
  if (norm.includes("mistral") || norm.includes("codestral")) {
    return <ApiKeyIcons.Mistral size={size} className={className} />;
  }
  if (norm.includes("openrouter")) {
    return <ApiKeyIcons.OpenRouter size={size} className={className} />;
  }
  if (norm.includes("perplexity") || norm.includes("sonar")) {
    return <ApiKeyIcons.Perplexity size={size} className={className} />;
  }
  if (norm.includes("cohere") || norm.includes("command")) {
    return <ApiKeyIcons.Cohere size={size} className={className} />;
  }
  if (norm.includes("ollama") || norm.includes("local") || norm.includes("custom")) {
    return <ApiKeyIcons.Ollama size={size} className={className} />;
  }
  if (norm.includes("cerebras")) {
    return <ApiKeyIcons.Cerebras size={size} className={className} />;
  }
  if (norm.includes("deepgram")) {
    return <ApiKeyIcons.Deepgram size={size} className={className} />;
  }
  if (norm.includes("elevenlabs") || norm.includes("eleven")) {
    return <ApiKeyIcons.ElevenLabs size={size} className={className} />;
  }
  if (norm.includes("github")) {
    return <ApiKeyIcons.GitHub size={size} className={className} />;
  }
  if (norm.includes("telegram")) {
    return <ApiKeyIcons.Telegram size={size} className={className} />;
  }

  return <ApiKeyIcons.GenericKey size={size} className={className} />;
}
