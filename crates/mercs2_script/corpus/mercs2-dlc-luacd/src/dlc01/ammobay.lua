local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("MrxSubtitle", false)
import("MrxSupportData", false)
import("MrxTutorialManager", false)
L0_1 = tEvents
if not L0_1 then
  L0_1 = {}
end
tEvents = L0_1
L0_1 = tMarker
if not L0_1 then
  L0_1 = {}
end
tMarker = L0_1
L0_1 = tHelpers
if not L0_1 then
  L0_1 = {}
end
tHelpers = L0_1
nCost = 250000
sLocalName = "munitions"

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2
  L3_2 = type(Object.GetHealth(A0_2))
  if L3_2 == "number" and 0 < L2_2 then
    L3_2 = tEvents
    L3_2[A0_2] = {}
    L3_2 = tMarker
    L3_2[A0_2] = {}
    L3_2 = tHelpers
    L4_2 = tHelpers[A0_2]
    if not L4_2 then
      L4_2 = {}
    end
    L3_2[A0_2] = L4_2
    L3_2 = tMarker[A0_2]
    L5_2 = A0_2
    L3_2.uMarker = Marker.AddDisc(L5_2, 4.5, 0, 255, 0, 0.02)
    L3_2 = Object.GetPosition
    L4_2 = A0_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    L6_2 = tHelpers[A0_2]
    L6_2.uBlipHelp = Pg.Spawn("location", L3_2, L4_2, L5_2, 0, false, true)
    L6_2 = Event.Create
    L7_2 = Event.ObjectHibernation
    L8_2 = {}
    L8_2[1] = A0_2
    L8_2[2] = "awake"
    
    function L9_2()
      local L0_3, L1_3
      SetupActivationEvents(A0_2)
    end
    
    L6_2(L7_2, L8_2, L9_2)
    L6_2 = tMarker[A0_2]
    L6_2.uIcon = Marker.AddBlip(tHelpers[A0_2].uBlipHelp, "HUD_Rearm_DLC", 30, 51, 204, 153, 255, 10, 150, 225)
    L6_2 = tMarker[A0_2]
    L6_2.uMiniMap = ("Blip_" .. tostring(A0_2))
    L6_2 = Minimap
    L6_2.AddObjectiveWithGuid(L6_2, tMarker[A0_2].uMiniMap, A0_2, 0, 0, 0, 51, 204, 153, 8, 8, "MiniMap_Icon_Rearm_DLC", false, nil, nil, 10)
  end
end

OnActivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  OnDeactivate(A0_2)
end

OnDeath = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = tEvents[A0_2]
  if L1_2 then
    L1_2 = pairs
    L2_2 = tEvents[A0_2]
    L1_2, L2_2, L3_2 = L1_2(L2_2)
    for L4_2, L5_2 in L1_2, L2_2, L3_2 do
      Event.Delete(L5_2)
    end
    L1_2 = tEvents
    L1_2[A0_2] = nil
  end
  L1_2 = tMarker[A0_2]
  if L1_2 then
    L1_2 = tMarker[A0_2].uMarker
    if L1_2 then
      Marker.Remove(tMarker[A0_2].uMarker)
    end
    L1_2 = tMarker[A0_2].uIcon
    if L1_2 then
      Marker.Remove(tMarker[A0_2].uIcon)
    end
    L1_2 = tMarker[A0_2].uMiniMap
    if L1_2 then
      L1_2 = Minimap
      L1_2.DeleteObjective(L1_2, tMarker[A0_2].uMiniMap)
      L1_2 = tMarker[A0_2]
      L1_2.uMiniMap = nil
    end
    L1_2 = tMarker
    L1_2[A0_2] = nil
  end
end

OnDeactivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L2_2 = tEvents[A0_2]
  L3_2 = Event.Create
  L4_2 = Event.ObjectProximity
  L5_2 = {}
  L5_2[1] = "vehicle"
  L5_2[2] = A0_2
  L5_2[3] = "<"
  L5_2[4] = 25
  L5_2[5] = false
  L5_2[6] = false
  
  function L6_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    L1_3 = tEvents[A0_3]
    L4_3 = {}
    L4_3[1] = 7
    L6_3 = {}
    L6_3[1] = true
    L1_3.uHideTutorial = Event.Create(Event.TimerRelative, L4_3, MrxTutorialManager.HideMessage, L6_3)
  end
  
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2.uTutorial = L3_2(L4_2, L5_2, L6_2, L7_2)
  L2_2 = tEvents[A0_2]
  L5_2 = {}
  L5_2[1] = L1_2
  L5_2[2] = A0_2
  L5_2[3] = "<"
  L5_2[4] = 5
  L5_2[5] = true
  L5_2[6] = false
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2.uProximity = Event.Create(Event.ObjectProximity, L5_2, Rearm, L7_2)
end

SetupActivationEvents = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L2_2 = Object.HasLabel(A0_2, "helipad")
  if L2_2 then
    MrxTutorialManager.StartTutorial("HeliRepairPad")
  else
  end
  L2_2 = Vehicle.GetDriver(A1_2[1])
  if L2_2 then
    L2_2 = Object.HasLabel(Vehicle.GetDriver(A1_2[1]), "hero")
    if L2_2 then
      L2_2 = Vehicle.IsFlying(A1_2[1])
      if not L2_2 then
        Sound.CueSound(A1_2[1], "ui_HUD_Pickup_Weapon_Large")
        Vehicle.RestoreAmmo(A1_2[1])
        MrxSubtitle.Add("[buildings.ammoBay.rearmed]")
        L2_2 = tEvents[A0_2]
        L5_2 = {}
        L5_2[1] = 30
        L7_2 = {}
        L7_2[1] = A0_2
        L2_2.uRecycle = Event.Create(Event.TimerRelative, L5_2, SetupActivationEvents, L7_2)
    end
  end
  else
    L2_2 = tEvents[A0_2]
    L5_2 = {}
    L5_2[1] = 3
    L7_2 = {}
    L7_2[1] = A0_2
    L2_2.uRecycle = Event.Create(Event.TimerRelative, L5_2, SetupActivationEvents, L7_2)
  end
end

Rearm = L0_1
