<script lang="ts">
	import type { Path } from '$app/types';
	import { resolve } from '$app/paths';
	import { localizeHref } from '#lib/paraglide/runtime.js';
	import { m } from '#lib/paraglide/messages.js';
	import { Badge } from '#lib/components/ui/badge/index.js';
	import { Button } from '#lib/components/ui/button/index.js';
	import * as Card from '#lib/components/ui/card/index.js';
	import ModerationAccess from '#lib/components/moderation-access.svelte';
	import { day, moment, outcomeWord, reasonWord, refusalWord } from '#lib/moderation-words.ts';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();

	const href = (path: string) => resolve(localizeHref(path) as Path);
	const effects: Record<string, () => string> = {
		none: m.moderation_effect_none,
		switched_off: m.moderation_effect_switched_off,
		hidden: m.moderation_effect_hidden
	};
	const refusedCase = $derived(form && 'refused' in form ? form.case : null);
	const refused = $derived(form && 'refused' in form ? refusalWord(form.refused ?? '') : null);
	const typed = (id: string) => (form && 'refused' in form && form.case === id ? form.reasons : '');
</script>

<svelte:head>
	<title>{m.moderation_title()} · {m.site_name()}</title>
</svelte:head>

<div class="mb-6 flex flex-wrap items-baseline justify-between gap-2">
	<h1 class="text-2xl font-semibold">{m.moderation_title()}</h1>
	{#if data.access === 'granted'}
		<a href={href('/moderation/figures')} class="text-sm underline-offset-4 hover:underline">
			{m.figures_link()}
		</a>
	{/if}
</div>

{#if data.access !== 'granted'}
	<ModerationAccess access={data.access} />
{:else}
	<p class="mb-6 text-muted-foreground">{m.moderation_intro()}</p>
	{#if form && 'decided' in form}
		<p role="status" class="mb-4">{m.moderation_decided()}</p>
	{/if}
	{#if data.cases.length === 0}
		<p>{m.moderation_empty()}</p>
	{/if}
	<div class="grid gap-6">
		{#each data.cases as waiting (waiting.kind === 'report' ? waiting.report : waiting.appeal)}
			{@const id = waiting.kind === 'report' ? waiting.report : waiting.appeal}
			<article aria-labelledby="case-{id}">
				<Card.Root>
					<Card.Header>
						<Card.Description class="flex flex-wrap gap-2">
							<Badge variant="secondary">
								{waiting.kind === 'report'
									? m.moderation_report()
									: waiting.appellant === 'author'
										? m.moderation_appeal_author()
										: m.moderation_appeal_reporter()}
							</Badge>
							<Badge variant="outline">{reasonWord(waiting.reason)}</Badge>
						</Card.Description>
						<Card.Title>
							<h2 id="case-{id}" class="text-lg">
								{waiting.title ?? m.moderation_untitled()}
							</h2>
						</Card.Title>
					</Card.Header>
					<Card.Content class="grid gap-3 text-sm">
						<p class="text-muted-foreground">
							{m.moderation_received({ date: moment(waiting.received_at) })} ·
							<strong>{m.moderation_due({ date: moment(waiting.due_at) })}</strong>
						</p>
						{#if waiting.kind === 'report'}
							<p>{m.moderation_version({ version: waiting.version })}</p>
							<p>{effects[waiting.effect]()}</p>
							{#if waiting.notice}
								<p>{m.moderation_notice(waiting.notice)}</p>
							{/if}
							{#if waiting.details}
								<div>
									<h3 class="font-medium">{m.moderation_details()}</h3>
									<p class="whitespace-pre-line">{waiting.details}</p>
								</div>
							{/if}
							{#if waiting.findings}
								<div>
									<h3 class="font-medium">{m.moderation_findings()}</h3>
									<p class="whitespace-pre-line">{waiting.findings}</p>
								</div>
							{/if}
						{:else}
							<div>
								<p>
									{m.moderation_appealed({
										moderator: waiting.decision.moderator,
										date: day(waiting.decision.decided_at)
									})}
									<strong>{outcomeWord(waiting.decision.outcome)}</strong>
								</p>
								<p class="whitespace-pre-line">{waiting.decision.reasons}</p>
							</div>
							<div>
								<h3 class="font-medium">{m.moderation_statement()}</h3>
								<p class="whitespace-pre-line">{waiting.statement}</p>
							</div>
						{/if}
						<p>
							<a
								href={href(`/moderation/items/${waiting.item}`)}
								class="underline underline-offset-4"
							>
								{m.moderation_history_link()}
							</a>
						</p>

						<form
							method="post"
							action={waiting.kind === 'report' ? '?/decideReport' : '?/decideAppeal'}
							class="grid gap-3 border-t pt-3"
						>
							<input type="hidden" name={waiting.kind} value={id} />
							{#if refusedCase === id && refused}
								<p role="alert" class="text-destructive">{refused}</p>
							{/if}
							<fieldset class="grid gap-2">
								<legend class="mb-1 font-medium">{m.moderation_outcome()}</legend>
								{#each waiting.kind === 'report' ? ['upheld', 'rejected'] : ['maintained', 'reversed'] as outcome (outcome)}
									<label class="flex gap-2">
										<input type="radio" name="outcome" value={outcome} required />
										{outcome === 'upheld'
											? m.outcome_upheld()
											: outcome === 'rejected'
												? m.outcome_rejected()
												: outcome === 'maintained'
													? m.outcome_maintained()
													: m.outcome_reversed()}
									</label>
								{/each}
							</fieldset>
							<label class="grid gap-1">
								{m.moderation_reasons()}
								<textarea
									name="reasons"
									required
									rows="3"
									class="rounded-md border bg-transparent px-3 py-2"
									value={typed(id)}></textarea>
							</label>
							<div>
								<Button type="submit">{m.moderation_decide()}</Button>
							</div>
						</form>
					</Card.Content>
				</Card.Root>
			</article>
		{/each}
	</div>
{/if}
