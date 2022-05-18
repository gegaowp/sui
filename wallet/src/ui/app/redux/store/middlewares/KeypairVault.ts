import { isAnyOf } from '@reduxjs/toolkit';

import { loadAccountFromStorage, setMnemonic } from '_redux/slices/account';
import { thunkExtras } from '_store/thunk-extras';

import type { Middleware } from '@reduxjs/toolkit';

const keypairVault = thunkExtras.keypairVault;
const matchUpdateMnemonic = isAnyOf(
    loadAccountFromStorage.fulfilled,
    setMnemonic
);

export const KeypairVault: Middleware = () => (next) => (action) => {
    if (matchUpdateMnemonic(action)) {
        if (action.payload) {
            keypairVault.mnemonic = action.payload;
        }
    }
    return next(action);
};
