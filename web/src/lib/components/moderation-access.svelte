<script lang="ts">
	// Why the administration pages cannot be shown, and how to get in.
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';

	let { access }: { access: string } = $props();

	const signIn = resolve(localizeHref('/sign-in?strong&return=moderation') as Path);
</script>

<Card.Root class="max-w-xl">
	<Card.Content class="grid gap-4">
		{#if access === 'not_a_moderator'}
			<p role="alert">{m.moderation_not_moderator()}</p>
		{:else}
			{#if access === 'passkey_required'}
				<h2 class="font-semibold">{m.moderation_passkey_title()}</h2>
				<p>{m.moderation_passkey()}</p>
			{:else}
				<p>{m.moderation_signed_out()}</p>
			{/if}
			<div>
				<Button href={signIn} data-sveltekit-reload>{m.moderation_sign_in()}</Button>
			</div>
		{/if}
	</Card.Content>
</Card.Root>
