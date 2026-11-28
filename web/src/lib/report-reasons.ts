// The reasons a comment may be reported for, the withdrawal list's codes
// (`docs/specs/catalogue-api.md`, Reports of comments), in the order the
// report form offers them.

export const REPORT_REASONS = [
	'personal_data',
	'sexual_content',
	'illegal_content',
	'rights',
	'unsuitable_content',
	'other'
] as const;
