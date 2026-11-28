<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { getLocale, localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import type { Received } from '#lib/server/profiles.ts';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const profile = $derived(data.profile);

	const href = (path: string) => resolve(localizeHref(path) as Path);
	const received = (entry: object): Received | null =>
		'suggestions' in entry ? (entry.suggestions as Received) : null;
</script>

<svelte:head>
	<title>{m.author_title({ pseudonym: profile.pseudonym })} · {m.site_name()}</title>
</svelte:head>

<header class="mb-6 grid gap-2">
	<h1 class="text-2xl font-semibold">{profile.pseudonym}</h1>
	{#if profile.editorial}
		<div><Badge variant="outline">{m.editorial()}</Badge></div>
	{/if}
	<p class="text-muted-foreground">
		{m.author_summary({ items: profile.items.length, families: profile.families })}
	</p>
	{#if data.own}
		<p class="max-w-xl text-sm">{m.author_own()}</p>
	{/if}
</header>

{#if profile.items.length === 0}
	<p>{m.author_none()}</p>
{:else}
	<ul class="grid gap-4 sm:grid-cols-2">
		{#each profile.items as entry (entry.item)}
			{@const suggestions = received(entry)}
			<li>
				<Card.Root lang={entry.metadata.language} class="h-full">
					<Card.Header>
						<Card.Title>
							<h2>
								<a href={href(`/items/${entry.item}`)} class="underline-offset-4 hover:underline">
									{entry.title}
								</a>
							</h2>
						</Card.Title>
						<Card.Description lang={getLocale()}>
							{entry.type === 'bank' ? m.type_bank() : m.type_path()}
						</Card.Description>
					</Card.Header>
					<Card.Content class="grid gap-2 text-sm" lang={getLocale()}>
						<p>{m.item_families({ count: entry.signals.families })}</p>
						{#if entry.selection}<div><Badge>{m.selection()}</Badge></div>{/if}
						{#if suggestions}
							<p>
								{m.author_suggestions({
									families: suggestions.families,
									answers: suggestions.answers
								})}
								{#if suggestions.answers > 0}
									·
									<a href={href('/account/suggestions')} class="underline">
										{m.suggestions_link()}
									</a>
								{/if}
							</p>
						{/if}
					</Card.Content>
				</Card.Root>
			</li>
		{/each}
	</ul>
{/if}
