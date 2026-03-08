# LOCO Friend/Profile Investigation Notes

## Confirmed surfaces

- `BLSYNC` returns a block/hidden-style revision payload.
- `BLMEMBER` returns profile summaries for the ids returned by `BLSYNC`.
- `SYNCMAINPF` exists in the macOS client binary and likely backs profile sync.
- `UPLINKPROF`, `MEMLIST`, `SYNCMEMT`, and `SETMEMTYPE` also exist in the macOS client binary.

## Confirmed read-only result

`loco-blocked --json` works today and returns:
- `user_id`
- `nickname`
- `profile_image_url`
- `full_profile_image_url`
- `suspended`
- `suspicion`
- `block_type`
- `is_plus`

This is not a full friend list replacement. It only proves that a friend/profile-related LOCO surface exists and is readable.

## Confirmed binary hints

From `KakaoTalk.app/Contents/MacOS/KakaoTalk`:
- `doSyncMainPfWithUsers:completion:`
- `doSetMemberTypeWithLinkId:chatId:memberIds:memberTypes:completion:`
- `initWithLinkId:chatId:memberIds:memberTypes:`
- `pfid=%@,r=r`
- `pfid=%@,r=n`
- `ct=d,pfid=%@`
- `ct=p,pfid=%@`
- `ct=me,pfid=%@`
- `profileToken`
- `profileType`
- `memberTypes`
- `privileges`

These strongly suggest:
- `SYNCMAINPF` is a profile-oriented read surface.
- `SYNCMEMT` / `SETMEMTYPE` are mutation-oriented member-type surfaces.
- `profileToken`, `profileType`, `multiProfileId`, and `accessPermit` matter for friend/profile modeling.

## Local data finding

The `Caches/Cache.db` file is only a `cfurl` HTTP cache.
It is useful for:
- token extraction
- cached REST URLs
- coarse response metadata

It is not the primary `NTUser` database.

The real Kakao local app data appears to live under:

- `~/Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac/`

There are at least two large extensionless `data` files there, and the app binary contains the full `NTUser` schema. This strongly suggests the real local user/chat database is stored there, likely encrypted or otherwise not directly readable as plain SQLite.

## Probe results so far

- `probe BLSYNC --body '{"r":0,"pr":0}' --json` succeeds.
- `probe SYNCMAINPF --body '{"ct":"me","pfid":405979308}' --json` reaches the method and returns `-203`.
- `probe SYNCMAINPF --body '{"ct":"d","pfid":153953637}' --json` reaches the method and returns `-203`.
- `probe MEMLIST` with empty or trivial bodies returns `-300`.
- `probe SYNCMEMT` with guessed `memberTypes` payload returns `-300`.

Interpretation:
- `SYNCMAINPF` is real, but the body is still incomplete.
- `MEMLIST` exists, but its required body shape is still unknown.
- `SYNCMEMT` should not be wired yet. The mutation semantics are still too uncertain.

## Operational caution

During probing, guessed mutation payloads can have real side effects.
Treat `SETMEMTYPE`, `SYNCMEMT`, `BLADDITEM`, and `BLDELITEM` as unsafe until semantics are proven on controlled targets.

## Next steps

1. Find the actual local DB/key path for `NTUser` and `NTChatRoom`.
2. Use that to map `userId`, `profileToken`, `profileType`, `multiProfileId`, and `accessPermit`.
3. Retry `SYNCMAINPF` with bodies derived from that mapping.
4. Keep mutation work out of user-facing commands until read-only profile fetch is proven.
