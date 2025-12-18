"""
LLM Provider Abstraction.
Decouples the application logic from specific LLM APIs (OpenAI, Anthropic, Gemini).
"""

from abc import ABC, abstractmethod
from typing import List, Dict, Any, Optional
import os


class LLMProvider(ABC):
    """
    Abstract Protocol for LLM Interaction.
    """

    @abstractmethod
    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        json_mode: bool = False,
    ) -> str:
        """
        Send messages to the LLM and return the content string.
        """
        pass


class OpenAIProvider(LLMProvider):
    def __init__(self, api_key: str = None, default_model: str = "gpt-4o"):
        import openai

        self.api_key = api_key or os.environ.get("OPENAI_API_KEY")
        if not self.api_key:
            raise ValueError("OpenAI API Key not found. Set OPENAI_API_KEY env var.")

        self.client = openai.Client(api_key=self.api_key)
        self.default_model = default_model

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        json_mode: bool = False,
    ) -> str:
        target_model = model or self.default_model
        params = {"model": target_model, "messages": messages, "temperature": 0.0}

        if json_mode:
            params["response_format"] = {"type": "json_object"}

        try:
            response = self.client.chat.completions.create(**params)
            return response.choices[0].message.content
        except Exception as e:
            # Wrap error or log it
            raise RuntimeError(f"OpenAI API Error: {e}") from e


# Placeholder / Future Providers
class AnthropicProvider(LLMProvider):
    def __init__(
        self, api_key: str = None, default_model: str = "claude-3-opus-20240229"
    ):
        import anthropic

        self.api_key = api_key or os.environ.get("ANTHROPIC_API_KEY")
        if not self.api_key:
            raise ValueError(
                "Anthropic API Key not found. Set ANTHROPIC_API_KEY env var."
            )

        self.client = anthropic.Anthropic(api_key=self.api_key)
        self.default_model = default_model

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        json_mode: bool = False,
    ) -> str:
        target_model = model or self.default_model

        # Extract system prompt if present (Anthropic treats it separately)
        system_prompt = None
        filtered_messages = []
        for msg in messages:
            if msg["role"] == "system":
                system_prompt = msg["content"]
            else:
                filtered_messages.append(msg)

        kwargs = {
            "model": target_model,
            "max_tokens": 4096,
            "messages": filtered_messages,
            "temperature": 0.0,
        }

        if system_prompt:
            kwargs["system"] = system_prompt

        try:
            response = self.client.messages.create(**kwargs)
            return response.content[0].text
        except Exception as e:
            raise RuntimeError(f"Anthropic API Error: {e}") from e


class ManualProvider(LLMProvider):
    """
    "Clipboard Mode" for using Web Subscriptions (ChatGPT Plus, Claude Pro).
    Prints the prompt to stdout and reads the response from stdin.
    """

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        json_mode: bool = False,
    ) -> str:
        import sys

        print("\n" + "=" * 60)
        print(f"MANUAL PROVIDER ({model or 'Human'})")
        print("=" * 60)

        # Display System Prompt if separate or part of messages
        for msg in messages:
            role = msg["role"].upper()
            content = msg["content"]
            print(f"\n[{role}]:")
            print("-" * 20)
            print(content)
            print("-" * 20)

        print("\n" + "=" * 60)
        print("INSTRUCTIONS:")
        print("1. Copy the prompts above into your LLM Web UI (e.g. Claude.ai).")
        if json_mode:
            print("2. Ensure the model outputs VALID JSON.")
        print("3. Paste the model's response below.")
        print("4. Press Enter, then Ctrl+Z (Windows) or Ctrl+D (Linux) to finish.")
        print("=" * 60 + "\n")

        # Read multi-line input
        output_lines = []
        try:
            while True:
                line = input()
                output_lines.append(line)
        except EOFError:
            pass

        return "\n".join(output_lines)


class ClaudeCLIProvider(LLMProvider):
    """
    Wraps the 'claude' command line tool.
    Assumes the user has already run 'claude login'.
    """

    def chat_completion(
        self,
        messages: List[Dict[str, str]],
        model: Optional[str] = None,
        json_mode: bool = False,
    ) -> str:
        import subprocess
        import shlex

        # Flatten messages into a single prompt string for the CLI
        # (The CLI typically takes a single prompt argument)
        full_prompt = ""
        for msg in messages:
            full_prompt += f"\n\n{msg['role'].upper()}: {msg['content']}"

        full_prompt += "\n\nASSISTANT:"

        # Construct command: claude "prompt" --print
        # '--print' or equivalent ensures output goes to stdout (we'll assume default behavior is interactive but -p forces print)
        # Based on help: claude [options] [command] [prompt]
        # We'll try passing the prompt as an arg.

        # Try piping via stdin which often triggers non-interactive mode
        # or at least passes the prompt correctly.
        # We also add '-p' just in case it supports 'print' via shorthand,
        # or we just rely on stdout capture.
        # Given 'claude [prompt]' starts a session, maybe piping avoids the TUI?

        # New approach: Pipe prompt to stdin.
        # cmd = ["claude", "--print"] if we knew the flag, but let's try just "claude" with input.

        # Actually, let's try the '-p' flag which is common for "print response".
        # If that fails, we might need a specific "non-interactive" argument.
        # But 'echo "hello" | claude' is the best bet for automation.

        cmd = ["claude"]

        try:
            result = subprocess.run(
                cmd,
                input=full_prompt,  # Pipe input
                capture_output=True,
                text=True,
                shell=True,
                encoding="utf-8",
                errors="replace",
                timeout=60,  # Prevent hanging if TUI waits for input
            )

            if result.returncode != 0:
                # If it fails, maybe try with argument again but simpler?
                # But let's return error for now.
                raise RuntimeError(
                    f"Claude CLI Error ({result.returncode}): {result.stderr}"
                )

            # Output might contain welcome message + response.
            # We can't easily strip it without reliable delimiters,
            # but usually piped output is cleaner.
            return result.stdout.strip()

        except Exception as e:
            raise RuntimeError(f"Failed to execute Claude CLI: {e}") from e


class manual_provider_placeholder:
    pass  # prevent re-definition error during edit


def get_provider(name: str, **kwargs) -> LLMProvider:
    if name.lower() == "openai":
        return OpenAIProvider(**kwargs)
    elif name.lower() == "anthropic":
        return AnthropicProvider(**kwargs)
    elif name.lower() == "claude-cli":
        return ClaudeCLIProvider()
    elif name.lower() == "manual":
        return ManualProvider()
    elif name.lower() == "gemini":
        return GeminiProvider()
    else:
        raise ValueError(f"Unknown provider: {name}")
