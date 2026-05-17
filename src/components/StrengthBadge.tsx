import { useI18n } from '../hooks/useI18n';

interface Props {
  level: 'weak' | 'medium' | 'strong' | string | undefined;
}

export function scorePassword(pwd: string): 'weak' | 'medium' | 'strong' {
  let score = 0;
  if (pwd.length >= 8) score++;
  if (pwd.length >= 12) score++;
  if (pwd.length >= 16) score++;
  if (/[a-z]/.test(pwd)) score++;
  if (/[A-Z]/.test(pwd)) score++;
  if (/\d/.test(pwd)) score++;
  if (/[^A-Za-z0-9]/.test(pwd)) score++;
  
  if (score <= 3) return 'weak';
  if (score <= 5) return 'medium';
  return 'strong';
}

export function StrengthBadge({ level }: Props) {
  const { t } = useI18n();
  
  if (!level) return null;
  
  return (
    <span className={`strength-badge ${level}`}>
      {t(`strength.${level}`)}
    </span>
  );
}
