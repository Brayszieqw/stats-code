/** Shared conservative direct-identifier detection for audit and previews. */

const SENSITIVE_FIELD = /^(?:name|full_name|first_name|last_name|given_name|family_name|(?:patient|participant|subject|person|contact)_name|姓名|phone|phone_number|mobile|mobile_number|telephone|电话|email|email_address|邮箱|address|street_address|home_address|住址|id_card|national_id|身份证|ssn|mrn|medical_record_number|date_of_birth|birth_date|dob|ip_address)$/i;

export function normalizeFieldName(value: string): string {
  return value.trim().toLocaleLowerCase().replace(/[^a-z0-9\u4e00-\u9fff]+/g, '_');
}

export function isSensitiveFieldName(value: string): boolean {
  return SENSITIVE_FIELD.test(normalizeFieldName(value));
}

export function looksLikeDirectIdentifier(value: string): boolean {
  const text = value.trim();
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(text)
    || /^1[3-9]\d{9}$/.test(text)
    || /^\d{17}[\dXx]$/.test(text);
}
