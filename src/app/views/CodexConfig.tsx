import { useEffect, useState } from "react";

import { errorMessage, managerApi } from "../../services/managerApi";
import { Icon } from "../icons";
import { useI18n } from "../i18n";
import { NavBar } from "../components";

const DEFAULT_URL = "https://aiapi.yxrobot.com/v1";

export function CodexConfig({ onBack }: { onBack: () => void }) {
  const { t } = useI18n();
  const [baseUrl, setBaseUrl] = useState(DEFAULT_URL);
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [apiKeySet, setApiKeySet] = useState(false);
  const [codexRunning, setCodexRunning] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    managerApi
      .getCodexGateway()
      .then((status) => {
        if (cancelled) return;
        setBaseUrl(status.baseUrl || DEFAULT_URL);
        setModel(status.model);
        setApiKeySet(status.apiKeySet);
        setCodexRunning(status.codexRunning);
      })
      .catch((cause) => {
        if (!cancelled) setError(errorMessage(cause));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function loadModels() {
    setError(null);
    setModelsLoading(true);
    try {
      const ids = await managerApi.listCodexGatewayModels({
        baseUrl,
        apiKey,
      });
      const next = model && !ids.includes(model) ? [model, ...ids] : ids;
      setModels(next);
      if (!model && next[0]) setModel(next[0]);
    } catch (cause) {
      setModels([]);
      setError(errorMessage(cause));
    } finally {
      setModelsLoading(false);
    }
  }

  async function save() {
    setError(null);
    setSaved(false);
    setSaving(true);
    try {
      const status = await managerApi.setCodexGateway({
        baseUrl,
        model,
        apiKey,
      });
      setBaseUrl(status.baseUrl);
      setModel(status.model);
      setApiKey("");
      setApiKeySet(status.apiKeySet);
      setCodexRunning(status.codexRunning);
      setSaved(true);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="pop">
      <NavBar title={t("nav.config")} onBack={onBack} />
      <div className="scroll view">
        <p className="desc" style={{ maxWidth: "46ch", marginTop: 18 }}>
          {t("config.desc")}
        </p>

        {codexRunning ? (
          <div className="banner warn" role="status" style={{ marginTop: 16 }}>
            <Icon name="alert" />
            <span>{t("config.running")}</span>
          </div>
        ) : null}
        {error ? (
          <div className="banner err" role="alert" style={{ marginTop: 16 }}>
            <Icon name="alert" />
            <span>{error}</span>
          </div>
        ) : null}
        {saved ? (
          <div className="banner info" role="status" style={{ marginTop: 16 }}>
            <Icon name="check" />
            <span>{t("config.saved")}</span>
          </div>
        ) : null}

        <div className="group" style={{ marginTop: 18 }}>
          <div className="group-h">{t("config.url")}</div>
          <div className="list">
            <div className="row" style={{ display: "block" }}>
              <input
                className="input mono"
                aria-label={t("config.url")}
                value={baseUrl}
                disabled={loading || saving || codexRunning}
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(event) => {
                  setSaved(false);
                  setBaseUrl(event.target.value);
                }}
              />
              <span className="rsub" style={{ display: "block", marginTop: 8 }}>
                {t("config.urlHint")}
              </span>
            </div>
          </div>
        </div>

        <div className="group">
          <div className="group-h">{t("config.apiKey")}</div>
          <div className="list">
            <div className="row" style={{ display: "block" }}>
              <input
                className="input mono"
                type="password"
                aria-label={t("config.apiKey")}
                value={apiKey}
                disabled={loading || saving || codexRunning}
                autoComplete="off"
                spellCheck={false}
                placeholder={
                  apiKeySet ? t("config.apiKeyKeep") : t("config.apiKeyPlaceholder")
                }
                onChange={(event) => {
                  setSaved(false);
                  setApiKey(event.target.value);
                }}
              />
              <span className="rsub" style={{ display: "block", marginTop: 8 }}>
                {t("config.apiKeyHint")}
              </span>
            </div>
          </div>
        </div>

        <div className="group">
          <div className="group-h">{t("config.model")}</div>
          <div className="list">
            <div className="row" style={{ display: "block" }}>
              {models.length > 0 ? (
                <select
                  className="input mono"
                  aria-label={t("config.model")}
                  value={model}
                  disabled={loading || saving || codexRunning}
                  onChange={(event) => {
                    setSaved(false);
                    setModel(event.target.value);
                  }}
                >
                  {models.map((id) => (
                    <option key={id} value={id}>
                      {id}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  className="input mono"
                  aria-label={t("config.model")}
                  value={model}
                  disabled={loading || saving || codexRunning}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  placeholder={t("config.modelPlaceholder")}
                  onChange={(event) => {
                    setSaved(false);
                    setModel(event.target.value);
                  }}
                />
              )}
              <button
                className="btn"
                type="button"
                style={{ marginTop: 10 }}
                disabled={loading || saving || modelsLoading || codexRunning}
                onClick={() => void loadModels()}
              >
                {modelsLoading ? t("config.modelsLoading") : t("config.modelsLoad")}
              </button>
              <span className="rsub" style={{ display: "block", marginTop: 8 }}>
                {t("config.modelHint")}
              </span>
            </div>
          </div>
        </div>

        <p className="desc" style={{ maxWidth: "46ch" }}>
          {t("config.restartHint")}
        </p>
        <button
          className="btn primary"
          type="button"
          disabled={loading || saving || codexRunning}
          onClick={() => void save()}
        >
          {saving ? t("config.saving") : t("config.save")}
        </button>
      </div>
    </div>
  );
}
