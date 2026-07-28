import("MrxCopterDrop")
import("MrxUtil")
tEvents = tEvents or {}

function OnActivate(uGuid)
  Debug.Printf("DropOff.OnActivate")
  tEvents[uGuid] = tEvents[uGuid] or {}
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
    StartTimer(uGuid, 0)
  end)
end

function OnDeactivate(uGuid)
  Debug.Printf("Dropoff.DeActivate")
  tEvents = tEvents or {}
  tEvents[uGuid] = tEvents[uGuid] or {}
  if tEvents[uGuid].CargoDrop then
    Event.Delete(tEvents[uGuid].CargoDrop)
    tEvents[uGuid].CargoDrop = nil
  end
  tEvents[uGuid] = nil
  NumDrops = nil
end

function StartTimer(uGuid, NumDrops)
  Debug.Printf("Dropoff.StartTimer")
  local iInterval = math.randf(30, 60)
  Debug.Printf(iInterval .. " is the interval")
  if tEvents[uGuid].CargoDrop then
    return
  else
    tEvents[uGuid].CargoDrop = Event.Create(Event.TimerRelative, {iInterval}, SetupCargoDrop, {uGuid, NumDrops})
  end
end

function SetupCargoDrop(uGuid, NumDrops)
  Debug.Printf("Dropoff.SetupCargoDrop")
  tEvents[uGuid].CargoDrop = nil
  x, y, z = Object.GetPosition(uGuid)
  local sFaction = MrxUtil.GetFaction(uGuid)
  Debug.Printf("Faction is " .. sFaction)
  if sFaction == "VZ" then
    HeloFaction = "VZ"
    tDropOffObjects = {
      "_port_containera_light",
      "_port_containerb_light",
      "_port_containerc_light",
      "_port_containerd_light",
      "M151 .50Cal (VZ)"
    }
  elseif sFaction == "Guerilla" then
    HeloFaction = "GR"
    tDropOffObjects = {
      "_port_containera",
      "_port_containerb",
      "_port_containerc",
      "_port_containerd",
      "M151 (MG) (GR)"
    }
  elseif sFaction == "China" then
    HeloFaction = "CH"
    tDropOffObjects = {
      "_port_containera",
      "_port_containerb",
      "_port_containerc",
      "_port_containerd",
      "NGLV (MG)"
    }
  elseif sFaction == "OC" then
    tDropOffObjects = {
      "_port_containera_light",
      "_port_containerb_light",
      "_port_containerc_light",
      "_port_containerd_light",
      "EXT"
    }
    HeloFaction = "OC"
  elseif sFaction == "Allied" then
    HeloFaction = "AL"
    tDropOffObjects = {
      "_port_containera",
      "_port_containerb",
      "_port_containerc",
      "_port_containerd",
      "HMMWV (Armored) (50Cal)"
    }
  elseif sFaction == "Pirate" then
    tDropOffObjects = {
      "_port_containera_light",
      "_port_containerb_light",
      "_port_containerc_light",
      "_port_containerd_light",
      "T300 (M60)"
    }
    HeloFaction = "PR"
  end
  sCargoTemplate = MrxUtil.GetRandomTableElement(tDropOffObjects)
  MrxCopterDrop.Create(HeloFaction, sCargoTemplate, x, y, z, false)
  NumDrops = NumDrops + 1
  if NumDrops < 1 then
    StartTimer(uGuid, NumDrops)
  end
end
