<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);

	const refusals: Record<string, () => string> = {
		answers_required: m.refused_answers_required,
		not_suggested: m.refused_not_suggested,
		strong_sign_in_required: m.refused_strong_sign_in_required,
		sharing_terms_required: m.refused_sharing_terms_required,
		share_quota_reached: m.refused_share_quota_reached
	};
	const formItem = $derived(form?.item ?? null);
	const code = $derived(form && 'refused' in form ? form.refused : null);
	const refused = $derived(code ? (refusals[code] ?? m.refused_other)() : null);
	const shared = $derived(form && 'shared' in form ? (form.shared ?? null) : null);
	const outcome = $derived(
		!shared
			? null
			: shared.outcome === 'published'
				? m.suggestions_published({ version: shared.version ?? 1 })
				: shared.outcome === 'held'
					? m.suggestions_held()
					: m.suggestions_refused()
	);
</script>

<svelte:head>
	<title>{m.suggestions_title()} · {m.site_name()}</title>
</svelte:head>

<h1 class="mb-1 text-2xl font-semibold">{m.suggestions_title()}</h1>
<p class="mb-6 max-w-xl text-muted-foreground">{m.suggestions_why()}</p>

{#if data.items.length === 0}
	<p>{m.suggestions_none()}</p>
{:else}
	<div class="grid max-w-xl gap-6">
		{#each data.items as item (item.item)}
			<Card.Root>
				<Card.Header>
					<Card.Title>
						<h2><a href={href(`/items/${item.item}`)} class="underline">{item.title}</a></h2>
					</Card.Title>
					<p class="text-sm text-muted-foreground">
						{m.suggestions_version({ version: item.version })}
					</p>
				</Card.Header>
				<Card.Content>
					<form method="post" action="?/accept" class="grid gap-4">
						<input type="hidden" name="item" value={item.item} />
						{#each item.questions as question (question.question)}
							<fieldset class="grid gap-2">
								<legend class="font-medium">
									{m.suggestions_question({ question: question.question })}{question.prompt
										? ` · ${question.prompt}`
										: ''}
								</legend>
								{#each question.answers as suggested (suggested.answer)}
									<label class="flex items-center gap-2">
										<input
											type="checkbox"
											name="accept"
											value={`${question.question}:${suggested.answer}`}
										/>
										<span>{suggested.answer}</span>
										<span class="text-sm text-muted-foreground">
											{m.suggestions_families({ count: suggested.families })}
										</span>
									</label>
								{/each}
							</fieldset>
						{/each}
						{#if formItem === item.item && refused}
							<p role="alert" class="text-destructive">{refused}</p>
						{/if}
						{#if formItem === item.item && outcome}
							<p role="status">{outcome}</p>
						{/if}
						<div>
							<Button type="submit">{m.suggestions_submit()}</Button>
						</div>
					</form>
				</Card.Content>
			</Card.Root>
		{/each}
	</div>
{/if}

<p class="mt-8">
	<a href={href('/account')} class="text-sm underline">{m.account_title()}</a>
</p>
