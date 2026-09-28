import anthropic from "@lobehub/icons-static-svg/icons/anthropic.svg";
import gemini from "@lobehub/icons-static-svg/icons/gemini-color.svg";
import openai from "@lobehub/icons-static-svg/icons/openai.svg";
import xai from "@lobehub/icons-static-svg/icons/xai.svg";
import { IconPlugConnected } from "@tabler/icons-react";
import type { Provider } from "./queries";

const LOGOS: Partial<Record<Provider, string>> = { anthropic, openai, google: gemini, xai };

/** The AI company's own mark; a plug for an OpenAI-compatible service. */
export function ProviderLogo({ provider, size = 16 }: { provider: Provider; size?: number }) {
  const logo = LOGOS[provider];
  if (!logo) return <IconPlugConnected size={size} stroke={1.75} className="shrink-0" aria-hidden="true" />;
  return <img src={logo} width={size} height={size} alt="" className="shrink-0" />;
}
