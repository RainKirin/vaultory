import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { UsernameConfig } from '../types';
import { useI18n } from '../hooks/useI18n';
import { usePersistedConfig } from '../hooks/usePersistedConfig';

const DEFAULT_CFG: UsernameConfig = {
  format: 'word1234',
};

interface Props {
  onApply: (username: string) => void;
  onClose: () => void;
  defaultConfig?: UsernameConfig;
}

export function UsernameGeneratorPanel({ onApply, onClose, defaultConfig }: Props) {
  const { t } = useI18n();
  const [cfg, setCfg] = usePersistedConfig<UsernameConfig>('username.config', defaultConfig ?? DEFAULT_CFG);
  const [preview, setPreview] = useState<string>('');

  const regen = useCallback(async () => {
    try {
      const p = await invoke<string>('generate_username_cmd', { config: cfg });
      setPreview(p);
    } catch (e) {
      console.error('Failed to generate username:', e);
    }
  }, [cfg]);

  useEffect(() => { regen(); }, [regen]);

  return (
    <div className="gen-panel">
      <div className="gen-preview">
        <code>{preview}</code>
        <button className="regen-btn" onClick={regen} title={t('regenerate')}>⭯</button>
      </div>
      <div className="gen-config">
        <label className="gen-config-row">
          <span>{t('gen.format')}</span>
          <select value={cfg.format} onChange={e => setCfg({ ...cfg, format: e.target.value })}>
            <option value="word1234">word1234</option>
            <option value="word_word">word_word</option>
            <option value="word.word">word.word</option>
          </select>
        </label>
      </div>
      <div className="gen-actions">
        <button onClick={onClose}>{t('cancel')}</button>
        <button className="primary" onClick={() => onApply(preview)}>{t('useThis')}</button>
      </div>
    </div>
  );
}
