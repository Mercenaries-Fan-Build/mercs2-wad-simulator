-- ═══ LESSON 1 ═══  The engine calls you. You never call the engine.
--
-- There is no main(). There is no "start" button. Nothing in this file runs because
-- it is in this file.
--
-- What actually happens: your script gets attached to an object in the world. When
-- something happens to THAT OBJECT, the engine looks up a function in your script BY
-- ITS EXACT NAME and calls it. Four names, that's the entire contract:
--
--     Init()                -- your module was loaded
--     OnActivate(uGuid)     -- your object streamed in
--     OnDeath(uGuid)        -- your object was destroyed
--     OnDeactivate(uGuid)   -- your object streamed out
--
-- Everything else you write is a helper that YOU call from one of those four.
--
-- Run this file. It will not crash. It will not warn you. It will do NOTHING:
--
--     cargo run -p mercs2_script --example lua_lab -- \
--         crates/mercs2_script/examples/lessons/01_the_engine_calls_you.lua
--
-- Read the report at the bottom. Then come back to STATION 1.


function onActivate(uGuid)
	Debug.Printf("guard " .. uGuid .. " reporting in")
end


-- ─── STATION 1 ────────────────────────────────────────────────────────────────
--
-- The lab told you: the engine looked for `OnActivate`, and you defined `onActivate`.
--
-- Lua does not know these names are special. `onActivate` is just a global variable
-- holding a function that nothing will ever read. There is no error to raise — nothing
-- is *wrong*. You simply wrote a function nobody calls, and the engine has no way to
-- guess that you meant something.
--
-- This is the shape of most of the pain in this API. Not crashes. Silence.
--
-- YOUR MOVE: capital O. Run it again.
--
-- Then try this, because it teaches the same lesson from the other side: rename it to
-- `OnActivated`, or `On_Activate`, or `OnActivate2`. Every one of them is silent too.
-- The engine is not matching loosely. It is doing the equivalent of a table lookup on
-- the string "OnActivate", and getting nil.
--
-- Once it prints, move on to 02_asleep_at_the_wheel.lua — where the name is right,
-- the code is right, and it STILL doesn't work.
-- ──────────────────────────────────────────────────────────────────────────────
