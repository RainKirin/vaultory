import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { PasswordGeneratorConfig, GeneratedPassword } from '../types';
import { useI18n } from '../hooks/useI18n';
import { usePersistedConfig } from '../hooks/usePersistedConfig';

const DEFAULT_CFG: PasswordGeneratorConfig = {
  length: 20,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  excludeAmbiguous: false,
};

interface Props {
  onApply: (password: string) => void;
  onClose: () => void;
  defaultConfig?: PasswordGeneratorConfig;
}

export function PasswordGeneratorPanel({ onApply, onClose, defaultConfig }: Props) {
  const { t } = useI18n();
  const [cfg, setCfg] = usePersistedConfig<PasswordGeneratorConfig>('pwgen.config', defaultConfig ?? DEFAULT_CFG);
  const [preview, setPreview] = useState<GeneratedPassword | null>(null);

  const regen = useCallback(async () => {
    try {
      const p = await invoke<GeneratedPassword>('generate_password_cmd', { config: cfg });
      setPreview(p);
    } catch (e) {
      console.error('Failed to generate password:', e);
    }
  }, [cfg]);

  useEffect(() => { regen(); }, [regen]);

  return (
    <div className="gen-panel">
      <div className="gen-preview">
        <code>{preview?.password}</code>
        <span className={`strength-badge ${preview?.strength}`}>
          {t(`strength.${preview?.strength ?? 'weak'}`)}
        </span>
        <button className="regen-btn" onClick={regen} title={t('regenerate')}>⭯</button>
      </div>
      <div className="gen-config">
        <label className="gen-config-row">
          <span>{t('gen.length')}: {cfg.length}</span>
          <input
            type="range"
            min={8}
            max={128}
            value={cfg.length}
            onChange={e => setCfg({ ...cfg, length: parseInt(e.target.value) })}
          />
        </label>
        <label className="gen-config-row">
          <input
            type="checkbox"
            checked={cfg.uppercase}
            onChange={e => setCfg({ ...cfg, uppercase: e.target.checked })}
          />
          <span>{t('gen.uppercase')}</span>
        </label>
        <label className="gen-config-row">
          <input
            type="checkbox"
            checked={cfg.lowercase}
            onChange={e => setCfg({ ...cfg, lowercase: e.target.checked })}
          />
          <span>{t('gen.lowercase')}</span>
        </label>
        <label className="gen-config-row">
          <input
            type="checkbox"
            checked={cfg.numbers}
            onChange={e => setCfg({ ...cfg, numbers: e.target.checked })}
          />
          <span>{t('gen.numbers')}</span>
        </label>
        <label className="gen-config-row">
          <input
            type="checkbox"
            checked={cfg.symbols}
            onChange={e => setCfg({ ...cfg, symbols: e.target.checked })}
          />
          <span>{t('gen.symbols')}</span>
        </label>
        <label className="gen-config-row">
          <input
            type="checkbox"
            checked={cfg.excludeAmbiguous}
            onChange={e => setCfg({ ...cfg, excludeAmbiguous: e.target.checked })}
          />
          <span>{t('gen.excludeAmbiguous')}</span>
        </label>
      </div>
      <div className="gen-actions">
        <button onClick={onClose}>{t('cancel')}</button>
        <button className="primary" onClick={() => preview && onApply(preview.password)}>{t('useThis')}</button>
      </div>
    </div>
  );
}
