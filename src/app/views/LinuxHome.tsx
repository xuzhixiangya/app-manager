import { useCallback, useEffect, useState } from "react";

import { errorMessage, managerApi } from "../../services/managerApi";
import type { LinuxCodexStatus } from "../../shared/types";
import { Ring, StatusBanner, TopBar } from "../components";
import { CodexGlyph, Icon } from "../icons";
import { useI18n } from "../i18n";

export function LinuxHome({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useI18n();
  const [status, setStatus] = useState<LinuxCodexStatus | null>(null);
  const [busy, setBusy] = useState<"install" | "launch" | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await managerApi.linuxStatus());
    } catch (cause) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const install = async () => {
    setBusy("install");
    setError(null);
    try {
      setStatus(await managerApi.linuxInstall());
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const launch = async () => {
    setBusy("launch");
    setError(null);
    try {
      await managerApi.linuxLaunch();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(null);
    }
  };

  const installed = status?.installed === true;
  const headline = installed
    ? t("linux.installed", { version: status?.version ?? "" })
    : t("home.none.title");

  return (
    <div className="pop">
      <TopBar />
      <div className="scroll">
        {error ? <StatusBanner tone="err">{error}</StatusBanner> : null}
        <section className="hero" style={{ marginTop: 16 }}>
          <Ring
            icon={installed ? "check" : "download"}
            variant={installed ? "success" : "accent"}
            spin={busy !== null}
          />
          <div className="headline">{headline}</div>
          <div className="sub">{busy === "install" ? t("linux.installing") : t("linux.sub")}</div>
          {status ? <div className="sub">{t("linux.arch", { arch: status.arch })}</div> : null}
        </section>
        <div className="actions">
          {installed ? (
            <button className="btn primary big" onClick={() => void launch()} disabled={busy !== null}>
              <CodexGlyph />
              {t("home.launch")}
            </button>
          ) : (
            <button
              className="btn primary big"
              onClick={() => void install()}
              disabled={busy !== null || status === null}
            >
              <Icon name="download" />
              {t("home.none.cta")}
            </button>
          )}
          <button className="btn ghost" onClick={onOpenSettings} disabled={busy !== null}>
            {t("nav.config")}
          </button>
        </div>
      </div>
    </div>
  );
}
