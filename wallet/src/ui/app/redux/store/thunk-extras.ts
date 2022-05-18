import KeypairVault from './KeypairVault';

export const thunkExtras = {
    keypairVault: new KeypairVault(),
};

export type ThunkExtras = typeof thunkExtras;
