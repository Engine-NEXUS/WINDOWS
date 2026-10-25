import React from "react";

export const McpIcons = {
  // Official CodeRabbit orange mark from coderabbit.ai/brand, kept intact.
  CodeRabbit: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" role="img" aria-label="CodeRabbit" {...props}>
      <image href="/brands/coderabbit-orange-mark.svg" width="24" height="24" />
    </svg>
  ),
  // Connector Link / Clip Header Icon
  Clip: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" />
    </svg>
  ),

  // Figma official emblem
  Figma: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 38 57" fill="none" {...props}>
      <path d="M19 28.5C19 23.2533 23.2533 19 28.5 19C33.7467 19 38 23.2533 38 28.5C38 33.7467 33.7467 38 28.5 38C23.2533 38 19 33.7467 19 28.5Z" fill="#1ABCFE"/>
      <path d="M0 47.5C0 42.2533 4.25329 38 9.5 38H19V47.5C19 52.7467 14.7467 57 9.5 57C4.25329 57 0 52.7467 0 47.5Z" fill="#0ACF83"/>
      <path d="M19 0V19H28.5C33.7467 19 38 14.7467 38 9.5C38 4.25329 33.7467 0 28.5 0H19Z" fill="#FF7262"/>
      <path d="M0 9.5C0 14.7467 4.25329 19 9.5 19H19V0H9.5C4.25329 0 0 4.25329 0 9.5Z" fill="#F24E1E"/>
      <path d="M0 28.5C0 33.7467 4.25329 38 9.5 38H19V19H9.5C4.25329 19 0 23.2533 0 28.5Z" fill="#A259FF"/>
    </svg>
  ),

  // Vercel triangle in rounded dark capsule
  Vercel: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <rect width="24" height="24" rx="5" fill="#000000" />
      <path d="M12 6L18 17H6L12 6Z" fill="#FFFFFF" />
    </svg>
  ),

  // Supabase double emerald polygon
  Supabase: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <path d="M13.4 2.1C13.1 1.6 12.4 1.5 12 1.9L2.8 11.2C2.3 11.7 2.6 12.6 3.4 12.6H11V21.9C11 22.5 11.8 22.8 12.2 22.3L21.3 12.9C21.8 12.4 21.5 11.5 20.8 11.5H13.4V2.1Z" fill="#3ECF8E" />
    </svg>
  ),

  // Stripe rounded purple S emblem
  Stripe: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <rect width="24" height="24" rx="5" fill="#635BFF" />
      <path d="M14.8 10.6C14.8 9.7 13.9 9.2 12.5 9.2C10.7 9.2 9.2 9.8 8.4 10.3L7.7 8.5C8.7 7.9 10.6 7.3 12.6 7.3C15.6 7.3 17.6 8.7 17.6 11.1C17.6 14.8 12.3 14.2 12.3 15.8C12.3 16.5 13 16.9 14.2 16.9C15.9 16.9 17.3 16.3 18.1 15.8L18.7 17.7C17.7 18.3 15.9 18.8 14 18.8C11 18.8 9.2 17.4 9.2 15C9.2 11.2 14.8 11.8 14.8 10.6Z" fill="#FFFFFF" />
    </svg>
  ),

  // Slack official 4-color hashtag
  Slack: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <path d="M5.5 10a2 2 0 1 1-2-2h2v2zm1 0a2 2 0 1 1 4 0v5a2 2 0 1 1-4 0v-5z" fill="#E01E5A" />
      <path d="M10 5.5a2 2 0 1 1 2-2v2h-2zm0 1a2 2 0 1 1 0 4H5a2 2 0 1 1 0-4h5z" fill="#36C5F0" />
      <path d="M18.5 14a2 2 0 1 1 2 2h-2v-2zm-1 0a2 2 0 1 1-4 0V9a2 2 0 1 1 4 0v5z" fill="#2EB67D" />
      <path d="M14 18.5a2 2 0 1 1-2 2v-2h2zm0-1a2 2 0 1 1 0-4h5a2 2 0 1 1 0 4h-5z" fill="#ECB22E" />
    </svg>
  ),

  // Netlify turquoise diamond
  Netlify: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <rect width="24" height="24" rx="5" fill="#00C7B7" />
      <path d="M12 4L4 12L12 20L20 12L12 4Z" fill="#0E1E25" opacity="0.9" />
      <path d="M12 7L7 12L12 17L17 12L12 7Z" fill="#00C7B7" />
    </svg>
  ),

  // Render official architectural curved ribbon mark
  Render: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" role="img" aria-label="Render" {...props}>
      <rect width="24" height="24" rx="5" fill="#141416" />
      <path
        d="M15.744 17.514h-3.082v-5.696c0-1.748-1.417-3.165-3.165-3.165H7.135V5.571h2.362c3.443 0 6.247 2.804 6.247 6.247v5.696z"
        fill="#46E3B7"
      />
    </svg>
  ),

  // GitHub Octocat
  GitHub: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor" {...props}>
      <path fillRule="evenodd" clipRule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" />
    </svg>
  ),

  // WhatsApp Official Emerald Speech Bubble with 45° Handset
  WhatsApp: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" role="img" aria-label="WhatsApp" {...props}>
      <circle cx="12" cy="12" r="10" fill="#25D366" />
      <path
        d="M12.04 4.5c-4.14 0-7.5 3.36-7.5 7.5 0 1.33.35 2.62 1 3.76L4.5 19.5l3.87-1.01c1.1.6 2.35.92 3.67.92 4.14 0 7.5-3.36 7.5-7.5s-3.36-7.41-7.5-7.41zm4.39 10.59c-.19.52-.96.99-1.39 1.05-.4.06-.9.08-1.46-.1-.34-.11-.78-.25-1.34-.49-2.34-1.01-3.86-3.41-3.98-3.57-.12-.16-.96-1.27-.96-2.43 0-1.15.6-1.72.82-1.95.21-.23.47-.29.62-.29.16 0 .31 0 .45.01.14.01.34-.05.53.4.19.47.66 1.6.72 1.72.06.12.1.26.02.42-.08.16-.12.26-.24.4-.12.14-.25.31-.36.42-.12.12-.25.25-.11.49.14.24.63 1.03 1.34 1.66.92.82 1.7 1.07 1.94 1.19.24.12.38.1.52-.06.14-.16.6-.7.76-.94.16-.24.32-.2.53-.12.21.08 1.34.63 1.57.75.23.12.38.18.44.27.06.11.06.63-.13 1.15z"
        fill="#FFFFFF"
      />
    </svg>
  ),

  // Swiggy Orange Food Emblem
  Swiggy: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <circle cx="12" cy="12" r="10" fill="#FC8019" />
      <path d="M12 5.5C9.5 5.5 7.5 7.5 7.5 10C7.5 13.2 11.5 18 12 18.5C12.5 18 16.5 13.2 16.5 10C16.5 7.5 14.5 5.5 12 5.5ZM12 11.5C11.2 11.5 10.5 10.8 10.5 10C10.5 9.2 11.2 8.5 12 8.5C12.8 8.5 13.5 9.2 13.5 10C13.5 10.8 12.8 11.5 12 11.5Z" fill="#FFFFFF" />
    </svg>
  ),

  Telegram: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <circle cx="12" cy="12" r="11" fill="#26A5E4" />
      <path d="M5.3 11.8 18.5 6.7c.6-.2 1.1.2.9 1l-2.2 10.2c-.1.7-.6.9-1.2.5l-3.4-2.5-1.7 1.6c-.2.2-.4.3-.6.3l.2-3.5 6.3-5.7c.3-.3-.1-.4-.4-.2l-7.8 4.9-3.3-1c-.7-.2-.7-.7 0-1z" fill="#fff" />
    </svg>
  ),
  Spotify: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <circle cx="12" cy="12" r="11" fill="#1DB954" />
      <path d="M5.5 9c4.7-1.4 9.9-.9 13.5 1.5M6.3 12c4-1.1 8.4-.6 11.6 1.4M7.2 14.8c3.4-.8 6.9-.4 9.7 1.2" stroke="#101a12" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  ),
  Amazon: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" {...props}>
      <rect width="24" height="24" rx="5" fill="#232F3E" />
      <path d="M12.6 14.4c-.8.7-1.5 1-2.5 1-1.4 0-2.2-.9-2.2-2.1 0-1.7 1.4-2.5 4.7-2.9v-.3c0-.7-.4-1.1-1.2-1.1-.8 0-1.3.4-1.4 1.1l-1.8-.2c.2-1.5 1.3-2.4 3.4-2.4 2.2 0 3 1 3 2.6v3.4c0 .6.2.9.5 1.3l-1.6 1.1c-.4-.4-.6-.8-.9-1.5zm0-2.8c-1.9.2-2.7.6-2.7 1.5 0 .6.4 1 1.1 1 1 0 1.6-.8 1.6-1.8v-.7z" fill="#fff" />
      <path d="M5.6 17.8c3.9 2.3 8.5 2.5 12.3-.2m-1.9.2 2-.4-.8 2" stroke="#FF9900" strokeWidth="1.25" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),

  // Filled Green Checkmark Badge
  CheckmarkBadge: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" role="img" aria-label="Connected" {...props}>
      <circle cx="12" cy="12" r="10.5" fill="#22c55e" />
      <path d="M7.5 12.5L10.5 15.5L16.5 9" stroke="#ffffff" strokeWidth="2.3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),

  // Checkmark icon
  Checkmark: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3.2" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <polyline points="20 6 9 17 4 12" />
    </svg>
  ),

  // Dotted Clock circle for Standby
  Clock: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeDasharray="2.5 2.5" strokeLinecap="round" strokeLinejoin="round" role="img" aria-label="Standby" {...props}>
      <circle cx="12" cy="12" r="9.5" />
      <polyline points="12 7.5 12 12 15 14.5" strokeDasharray="none" />
    </svg>
  ),

  // Radial multi-tick spinner
  RadialSpinner: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" role="img" aria-label="Connecting" {...props}>
      <line x1="12" y1="2" x2="12" y2="5" opacity="1.0" />
      <line x1="19.07" y1="4.93" x2="16.95" y2="7.05" opacity="0.88" />
      <line x1="22" y1="12" x2="19" y2="12" opacity="0.75" />
      <line x1="19.07" y1="19.07" x2="16.95" y2="16.95" opacity="0.62" />
      <line x1="12" y1="22" x2="12" y2="19" opacity="0.5" />
      <line x1="4.93" y1="19.07" x2="7.05" y2="16.95" opacity="0.38" />
      <line x1="2" y1="12" x2="5" y2="12" opacity="0.25" />
      <line x1="4.93" y1="4.93" x2="7.05" y2="7.05" opacity="0.12" />
    </svg>
  ),

  // Information Encrypted Badge
  InfoShield: (props: React.SVGProps<SVGSVGElement>) => (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" {...props}>
      <circle cx="12" cy="12" r="10" />
      <line x1="12" y1="16" x2="12" y2="12" />
      <line x1="12" y1="8" x2="12.01" y2="8" />
    </svg>
  ),
};
