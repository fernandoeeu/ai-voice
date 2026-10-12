/** What the browser may learn about a failed Alexa call: a category and a message that is safe to show. */
export type AlexaFailure = {
	readonly kind: 'session' | 'device' | 'speech' | 'config' | 'unexpected';
	readonly message: string;
};

export type Outcome<A> =
	| { readonly ok: true; readonly value: A }
	| { readonly ok: false; readonly failure: AlexaFailure };
