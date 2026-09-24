import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { managerApi } from "../../services/managerApi";
import { I18nProvider } from "../i18n";
import { ThemeProvider } from "../theme";
import { CodexConfig } from "./CodexConfig";

vi.mock("../../services/managerApi", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../services/managerApi")>();
  return {
    ...actual,
    managerApi: {
      getCodexGateway: vi.fn(),
      setCodexGateway: vi.fn(),
      listCodexGatewayModels: vi.fn(),
    },
  };
});

const api = vi.mocked(managerApi);

function renderConfig() {
  return render(
    <ThemeProvider>
      <I18nProvider>
        <CodexConfig onBack={vi.fn()} />
      </I18nProvider>
    </ThemeProvider>,
  );
}

describe("Codex gateway config", () => {
  beforeEach(() => {
    localStorage.setItem("cam.lang", "zh-CN");
    api.getCodexGateway.mockReset();
    api.setCodexGateway.mockReset();
    api.listCodexGatewayModels.mockReset();
    api.listCodexGatewayModels.mockResolvedValue([]);
  });

  it("saves the gateway, key, and model, then clears the key field", async () => {
    const user = userEvent.setup();
    api.getCodexGateway.mockResolvedValue({
      baseUrl: "https://aiapi.yxrobot.com/v1",
      model: "",
      apiKeySet: false,
      codexRunning: false,
    });
    api.setCodexGateway.mockResolvedValue({
      baseUrl: "https://aiapi.yxrobot.com/v1",
      model: "gpt-5.4",
      apiKeySet: true,
      codexRunning: false,
    });

    renderConfig();
    const key = await screen.findByLabelText("API Key");
    await user.type(key, "sk-employee");
    await user.type(screen.getByLabelText("默认模型"), "gpt-5.4");
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(api.setCodexGateway).toHaveBeenCalledWith({
        baseUrl: "https://aiapi.yxrobot.com/v1",
        model: "gpt-5.4",
        apiKey: "sk-employee",
      }),
    );
    expect(screen.getByLabelText("API Key")).toHaveValue("");
    expect(screen.getByText("已保存。请完全退出 Codex 后重新打开，新配置才会生效。")).toBeInTheDocument();
  });

  it("blocks saving while Codex is running", async () => {
    api.getCodexGateway.mockResolvedValue({
      baseUrl: "https://aiapi.yxrobot.com/v1",
      model: "gpt-5.4",
      apiKeySet: true,
      codexRunning: true,
    });
    renderConfig();
    expect(await screen.findByRole("button", { name: "保存" })).toBeDisabled();
    expect(screen.getByText(/请先完全退出/)).toBeInTheDocument();
  });

  it("loads the gateway model list and lets the user pick one", async () => {
    const user = userEvent.setup();
    api.getCodexGateway.mockResolvedValue({
      baseUrl: "https://aiapi.yxrobot.com/v1",
      model: "openai/gpt-5.4",
      apiKeySet: true,
      codexRunning: false,
    });
    api.listCodexGatewayModels.mockResolvedValue([
      "claude-sonnet",
      "openai/gpt-5.4",
    ]);

    renderConfig();
    await user.click(await screen.findByRole("button", { name: "加载可用模型" }));

    const picker = await screen.findByRole("combobox", { name: "默认模型" });
    expect(picker).toHaveValue("openai/gpt-5.4");
    await user.selectOptions(picker, "claude-sonnet");
    expect(picker).toHaveValue("claude-sonnet");
  });
});
