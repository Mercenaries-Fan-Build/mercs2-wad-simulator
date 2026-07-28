function Init()
  uEvent = uEvent or {}
  
  uHumanFilter = uHumanFilter or ObjectFilter.Create()
  uVehicleFilter = uVehicleFilter or ObjectFilter.Create()
  ObjectFilter.SetFilter(uHumanFilter, "human")
  ObjectFilter.SetFilter(uVehicleFilter, "vehicle")
end

function Deinit()
  uHumanFilter = nil
  uVehicleFilter = nil
end

function OnActivate(uGuid, uOwner, nArg)
  Debug.Printf("@@@@@@ ACTIVATING: ", uOwner, nArg)
  Object.PlayMaterialAnimation(uGuid, "global_weapon_c4land_60thsec", true)
  if nArg == 2 then
    Debug.Printf("Waiting for proximity")
    uEvent[uGuid] = Event.Create(Event.ObjectProximity, {
      uHumanFilter,
      uGuid,
      "<",
      1,
      false,
      false
    }, Object.Kill, {uGuid})
  else
  end
end

function OnDeactivate(uGuid, nArg)
  Object.StopMaterialAnimation(uGuid, "global_weapon_c4land_60thsec")
  Event.Delete(uEvent[uGuid])
  uEvent[uGuid] = nil
end

function OnDeath(uGuid)
  Sound.CueSound(uGuid, "wpn_bomb_timer_01_finalstage")
  uEvent[uGuid] = nil
  local x, y, z = Object.GetPosition(uGuid)
  if x then
    if Object.HasLabel(uGuid, "HumanMine") then
      Event.Create(Event.TimerRelative, {0.75}, Explode, {
        uGuid,
        x,
        y,
        z,
        "human"
      })
    else
      Event.Create(Event.TimerRelative, {0.25}, Explode, {
        uGuid,
        x,
        y,
        z,
        "veh"
      })
    end
  end
end

function Explode(uGuid, x, y, z, sType)
  Sound.StopSound(uGuid, "wpn_bomb_timer_01_finalstage")
  if sType == "human" then
    Pg.Spawn("Explosion (Grenade)", x, y, z)
  elseif sType == "veh" then
    Pg.Spawn("Explosion (AT Mine)", x, y, z)
  else
    Pg.Spawn("Explosion (Water Mine)", x, y, z)
  end
end
