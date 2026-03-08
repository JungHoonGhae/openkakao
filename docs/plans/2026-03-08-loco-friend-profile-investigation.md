# LOCO Friend/Profile Investigation

## Current Position

- `BLSYNC` and `BLMEMBER` are confirmed read-only LOCO surfaces for blocked or hidden-style members.
- `SYNCMAINPF`, `UPLINKPROF`, `SYNCMEMT`, and `SETMEMTYPE` exist in the KakaoTalk macOS binary, but their exact request bodies are not fully mapped yet.
- `friends`, `profile`, `favorite`, `hide`, and related commands should not be switched to LOCO until the payload semantics are concrete.

## Confirmed Read-Only Result

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

## Confirmed Binary Hints

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
- `PROFILELISTREVISION:%@`
- `DESIGNATEDFRIENDSREVISION:%@`
- `drawerUserInfoSecureKey:%@`

These strongly suggest:
- `SYNCMAINPF` is a profile-oriented read surface.
- `SYNCMEMT` / `SETMEMTYPE` are mutation-oriented member-type surfaces.
- `profileToken`, `profileType`, `multiProfileId`, and `accessPermit` matter for friend/profile modeling.

## Confirmed Local Hints

- `Cache.db` contains cached REST profile requests such as:
  - `/mac/profile3/friend.json?accessPermit=...&chatId=...&id=...`
  - `/mac/profile3/friends.json?category=action&ids=[...]`
  - `/mac/profile/designated_friends.json`
- Local plist files contain:
  - `PROFILELISTREVISION:*`
  - `DESIGNATEDFRIENDSREVISION:*`
  - `kLocoBlockFriendsSyncKey`
  - `kLocoBlockChannelsSyncKey`
- The KakaoTalk process currently exposes `Cache.db` and `httpstorages.sqlite` via `lsof`, but not the large opaque files in `Application Support`. Those files may still back the real `NTUser` storage, but they are not directly observable from this session.

## Current Local Storage Assessment

`Cache.db` is confirmed to be a `cfurl` cache store.  
It is useful for:
- token extraction
- cached REST URLs
- coarse response metadata

It is not the primary `NTUser` database.

The real Kakao local app data appears to live under:

- `~/Library/Containers/com.kakao.KakaoTalkMac/Data/Library/Application Support/com.kakao.KakaoTalkMac/`

There are at least two large extensionless `data` files there, and the app binary contains the full `NTUser` schema. This strongly suggests the real local user/chat database is stored there, likely encrypted or otherwise not directly readable as plain SQLite.

## Probe Results So Far

- `probe BLSYNC --body '{"r":0,"pr":0}' --json` succeeds.
- `probe SYNCMAINPF --body '{"ct":"me","pfid":405979308}' --json` reaches the method and returns `-203`.
- `probe SYNCMAINPF --body '{"ct":"d","pfid":153953637}' --json` reaches the method and returns `-203`.
- `probe MEMLIST` with empty or trivial bodies returns `-300`.
- `probe SYNCMEMT` with guessed `memberTypes` payload returns `-300`.

Interpretation:
- `SYNCMAINPF` is real, but the body is still incomplete.
- `MEMLIST` exists, but its required body shape is still unknown.
- `SYNCMEMT` should not be wired yet. The mutation semantics are still too uncertain.

## Developer Tooling

- Hidden command added on the branch:
  - `openkakao-rs profile-hints`
  - `openkakao-rs profile-hints --json`
- New narrowing mode on the branch:
  - `openkakao-rs profile-hints --local-graph --user-id <id> --json`
  - generates candidate `SYNCMAINPF` bodies from local graph + cached REST hints
- It reports:
  - cached profile/designated-friends request hints from `Cache.db`
  - parsed `userId`, `chatId`, `accessPermit`, and `category`
  - best observed `PROFILELISTREVISION` / `DESIGNATEDFRIENDSREVISION`
  - block sync flags from KakaoTalk plist state

## Latest Probe Narrowing

- For `Christine` (`user_id=32262572`, `account_id=54560688`) the generated candidates were:
  - `ct=d,pfid=32262572,chatId=382367313744175,accessPermit=...`
  - `ct=p,pfid=32262572,chatId=382367313744175,accessPermit=...`
  - `ct=d,pfid=54560688,chatId=382367313744175,accessPermit=...`
  - `ct=p,pfid=54560688,chatId=382367313744175,accessPermit=...`
- With the improved `probe` output, the first candidate is now confirmed to:
  - reach `SYNCMAINPF`
  - return a direct response with `status=-203`
  - interleave an unrelated `BLSYNC` push packet
- This means the naive `ct + pfid(user/account) + chatId + accessPermit` shape is not sufficient yet, but it is not rejected at the transport level either. The remaining missing input is likely another profile discriminator such as `profileType`, relation flag, or a different `pfid` source.

## Operational Caution

During probing, guessed mutation payloads can have real side effects.
Treat `SETMEMTYPE`, `SYNCMEMT`, `BLADDITEM`, and `BLDELITEM` as unsafe until semantics are proven on controlled targets.

## Next Step

Use the `profile-hints` output to drive more precise `SYNCMAINPF` probes instead of guessing bodies blindly. The most likely missing input is a local profile discriminator such as `pfid`, `profileType`, or a relation between `userId` and `accessPermit/chatId`.
