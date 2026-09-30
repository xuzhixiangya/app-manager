import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { managerApi } from "../../services/managerApi";
import type { LinuxCodexStatus } from "../../shared/types";
import { I18nProvider } from "../i18n";
import { ThemeProvider } from "../theme";
import { Home } from "./Home";

vi.mock("../../services/managerApi", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../services/managerApi")>();
  return {
    ...actual,
    managerApi: {
      ...actual.managerApi,
      getHostArchitecture: vi.fn(async () => "x64"),
      linuxStatus: vi.fn(),
      linuxInstall: vi.fn(),
      linuxLaunch: vi.fn(),
    },
  };
});

const api = vi.mocked(managerApi);

function setPlatform(platform: string) {
  Object.defineProperty(navigator, "platform", {
    configurable: true,
    value: platform,
  });
}

function renderHome() {
  return render(
    <ThemeProvider>
      <I18nProvider>
        <Home onOpenSettings={vi.fn()} />
      </I18nProvider>
    </ThemeProvider>,
  );
}

const MISSING: LinuxCodexStatus = {
  installed: false,
  version: null,
  arch: "amd64",
};

const INSTALLED: LinuxCodexStatus = {
  installed: true,
  version: "26.810.41047",
  arch: "amd64",
};

describe("LinuxHome", () => {
  beforeEach(() => {
    localStorage.setItem("cam.lang", "zh-CN");
    setPlatform("Linux x86_64");
    api.linuxStatus.mockResolvedValue(MISSING);
    api.linuxInstall.mockResolvedValue(INSTALLED);
    api.linuxLaunch.mockResolvedValue(undefined);
  });

  it("offers the official Ubuntu install when Codex is missing", async () => {
    const user = userEvent.setup();
    renderHome();
    expect(await screen.findByText("未检测到 Codex")).toBeTruthy();
    expect(screen.getByText("这台电脑是 amd64，将安装对应的官方包。")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "安装 Codex" }));
    expect(api.linuxInstall).toHaveBeenCalledOnce();
    expect(await screen.findByText("已安装 26.810.41047")).toBeTruthy();
  });

  it("launches the installed desktop app", async () => {
    const user = userEvent.setup();
    api.linuxStatus.mockResolvedValue(INSTALLED);
    renderHome();
    await user.click(await screen.findByRole("button", { name: "启动 Codex" }));
    expect(api.linuxLaunch).toHaveBeenCalledOnce();
  });
});
