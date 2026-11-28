import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	LIBRARY_API: {
		description: "The library service's address, whose catalogue API the website reads.",
		schema: (value) => value || 'http://127.0.0.1:8080'
	}
});
