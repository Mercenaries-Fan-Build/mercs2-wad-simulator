local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("MrxUtil", false)
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
L0_1 = tPrices
if not L0_1 then
  L0_1 = {}
end
tPrices = L0_1
L0_1 = tHelpers
if not L0_1 then
  L0_1 = {}
end
tHelpers = L0_1
sLocalName = "munitions"

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2, L20_2, L21_2, L22_2
  L3_2 = type(Object.GetHealth(A0_2))
  if L3_2 == "number" and 0 < L2_2 then
    L3_2 = tEvents
    L4_2 = tEvents[A0_2]
    if not L4_2 then
      L4_2 = {}
    end
    L3_2[A0_2] = L4_2
    L3_2 = tMarker
    L4_2 = tMarker[A0_2]
    if not L4_2 then
      L4_2 = {}
    end
    L3_2[A0_2] = L4_2
    L3_2 = tPrices
    L4_2 = tPrices[A0_2]
    if not L4_2 then
      L4_2 = {}
    end
    L3_2[A0_2] = L4_2
    L3_2 = tHelpers
    L4_2 = tHelpers[A0_2]
    if not L4_2 then
      L4_2 = {}
    end
    L3_2[A0_2] = L4_2
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
      local L0_3, L1_3, L2_3
      L0_3 = Object.IsAlive(A0_2)
      if L0_3 then
        SetupActivationEvents(A0_2)
      end
    end
    
    L6_2(L7_2, L8_2, L9_2)
    L6_2 = tMarker[A0_2]
    L6_2.uIcon = Marker.AddBlip(tHelpers[A0_2].uBlipHelp, "HUD_Repair_DLC", 30, 51, 204, 153, 255, 10, 150, 225)
    L6_2 = tMarker[A0_2]
    L6_2.uMiniMap = ("Blip_" .. tostring(A0_2))
    L6_2 = Minimap
    L6_2.AddObjectiveWithGuid(L6_2, tMarker[A0_2].uMiniMap, A0_2, 0, 0, 0, 51, 204, 153, 8, 8, "MiniMap_Icon_Repair_DLC", false, nil, nil, 10)
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
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2
  L1_2 = tPrices[A0_2]
  L1_2.nCost = 500000
  L2_2 = "hp_veh_spawn"
  L3_2 = Object.GetHardpointPosition
  L4_2 = A0_2
  L5_2 = "hp_veh_spawn"
  L3_2, L4_2, L5_2 = L3_2(L4_2, L5_2)
  L6_2 = {}
  L6_2[1] = 7.064171
  L6_2[2] = 1.165347
  L7_2 = tEvents[A0_2]
  L8_2 = Event.Create
  L9_2 = Event.ObjectProximity
  L10_2 = {}
  L10_2[1] = "vehicle"
  L10_2[2] = A0_2
  L10_2[3] = "<"
  L10_2[4] = 25
  L10_2[5] = false
  L10_2[6] = false
  
  function L11_2(A0_3)
    local L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    L1_3 = tEvents[A0_3]
    L4_3 = {}
    L4_3[1] = 7
    L6_3 = {}
    L6_3[1] = true
    L1_3.uHideTutorial = Event.Create(Event.TimerRelative, L4_3, MrxTutorialManager.HideMessage, L6_3)
  end
  
  L12_2 = {}
  L12_2[1] = A0_2
  L7_2.uTutorial = L8_2(L9_2, L10_2, L11_2, L12_2)
  L7_2 = tHelpers[A0_2]
  L7_2.uDiscHelp = Pg.Spawn("location", L3_2, L4_2, L5_2, 0, false, true)
  L7_2 = tMarker[A0_2].uMarker
  if L7_2 then
    Marker.Remove(tMarker[A0_2].uMarker)
    L7_2 = tMarker[A0_2]
    L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 0, 255, 0, 0.05)
  else
    L7_2 = tMarker[A0_2]
    L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 0, 255, 0, 0.05)
  end
  PlayerOutside(A0_2)
  L7_2 = tEvents[A0_2]
  L10_2 = {}
  L10_2[1] = L1_2
  L10_2[2] = L3_2
  L10_2[3] = L4_2
  L10_2[4] = L5_2
  L10_2[5] = "<"
  L10_2[6] = 5
  L10_2[7] = true
  L10_2[8] = false
  L12_2 = {}
  L12_2[1] = A0_2
  L7_2.uProximity = Event.Create(Event.ObjectProximity, L10_2, InitiateRepair, L12_2)
end

SetupActivationEvents = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2
  L2_2 = Vehicle.GetDriver(A1_2[1])
  L3_2 = MrxPmc.GetCashQty()
  L6_2 = (Object.GetHealth(A1_2[1]) / Object.GetMaxHealth(A1_2[1])) * 100
  if L2_2 then
    L7_2 = Object.HasLabel(L2_2, "hero")
    if L7_2 then
      L7_2 = Vehicle.IsFlying(A1_2[1])
      if not L7_2 then
        L7_2 = tEvents[A0_2]
        L10_2 = {}
        L10_2[1] = A1_2[1]
        L12_2 = {}
        L12_2[1] = A1_2[1]
        L12_2[2] = 1.5
        L12_2[3] = true
        L7_2.uCleanUp = Event.Create(Event.ObjectDeath, L10_2, Object.FadeOut, L12_2)
        L7_2 = tPrices[A0_2].nCost
        if L3_2 >= L7_2 then
          if L6_2 < 99 then
            L7_2 = tMarker[A0_2].uMarker
            if L7_2 then
              Marker.Remove(tMarker[A0_2].uMarker)
              L7_2 = tMarker[A0_2]
              L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 255, 255, 0, 0.05)
            end
            Sound.CueSound(A1_2[1], "ui_HUD_Pickup_Weapon_Large")
            StartCinematic(A0_2, A1_2[1], L2_2)
            L7_2 = tEvents[A0_2]
            L10_2 = {}
            L10_2[1] = Player.GetAnyCharacter()
            L10_2[2] = A0_2
            L10_2[3] = ">"
            L10_2[4] = 20
            L10_2[5] = true
            L10_2[6] = false
            L12_2 = {}
            L12_2[1] = A0_2
            L7_2.uRecycle = Event.Create(Event.ObjectProximity, L10_2, SetupActivationEvents, L12_2)
          else
            L7_2 = tMarker[A0_2].uMarker
            if L7_2 then
              Marker.Remove(tMarker[A0_2].uMarker)
              L7_2 = tMarker[A0_2]
              L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 255, 255, 0, 0.05)
            end
            MrxSubtitle.Add("[buildings.repairBay.undamaged]")
            L7_2 = tEvents[A0_2]
            L10_2 = {}
            L10_2[1] = 15
            L12_2 = {}
            L12_2[1] = A0_2
            L7_2.uRecycle = Event.Create(Event.TimerRelative, L10_2, SetupActivationEvents, L12_2)
          end
        else
          L7_2 = tMarker[A0_2].uMarker
          if L7_2 then
            Marker.Remove(tMarker[A0_2].uMarker)
            L7_2 = tMarker[A0_2]
            L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 255, 255, 0, 0.05)
          end
          MrxSubtitle.Add("[buildings.repairBay.noFunds]")
          L7_2 = tEvents[A0_2]
          L10_2 = {}
          L10_2[1] = 5
          L12_2 = {}
          L12_2[1] = A0_2
          L7_2.uRecycle = Event.Create(Event.TimerRelative, L10_2, SetupActivationEvents, L12_2)
        end
    end
  end
  else
    L7_2 = tMarker[A0_2].uMarker
    if L7_2 then
      Marker.Remove(tMarker[A0_2].uMarker)
      L7_2 = tMarker[A0_2]
      L7_2.uMarker = Marker.AddDisc(tHelpers[A0_2].uDiscHelp, 3, 255, 255, 0, 0.05)
    end
    L7_2 = tEvents[A0_2]
    L10_2 = {}
    L10_2[1] = 5
    L12_2 = {}
    L12_2[1] = A0_2
    L7_2.uRecycle = Event.Create(Event.TimerRelative, L10_2, SetupActivationEvents, L12_2)
  end
end

InitiateRepair = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L3_2 = Player.GetPrimaryPlayer()
  L4_2 = Player.GetCamera(L3_2)
  L5_2 = {}
  L5_2[1] = -15.5
  L5_2[2] = 14.5
  L5_2[3] = -30.5
  L6_2 = {}
  L9_2 = 1.165347
  L6_2[1] = 7.064171
  L6_2[2] = 0
  L6_2[3] = L9_2
  L7_2 = Object.GetPosition
  L8_2 = A0_2
  L7_2, L8_2, L9_2 = L7_2(L8_2)
  Player.SetCinematicMode(L3_2, true)
  Camera.SetPosition(L4_2, (L7_2 + L5_2[1]), (L8_2 + L5_2[2]), (L9_2 + L5_2[3]))
  Camera.SetLookAt(L4_2, A0_2, false)
  L10_2 = tEvents[A0_2]
  L13_2 = {}
  L13_2[1] = 1
  L15_2 = {}
  L15_2[1] = A0_2
  L15_2[2] = A1_2
  L15_2[3] = L3_2
  L15_2[4] = L4_2
  L10_2.uRepairReady = Event.Create(Event.TimerRelative, L13_2, Repair, L15_2)
end

StartCinematic = L0_1

function L0_1(A0_2, A1_2, A2_2, A3_2)
  local L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L4_2 = Object.GetParent(A1_2)
  Object.CloseGate(A0_2)
  L7_2 = {}
  L7_2[1] = 1
  L9_2 = {}
  L9_2[1] = A1_2
  L9_2[2] = A2_2
  L9_2[3] = true
  Event.Create(Event.TimerRelative, L7_2, Vehicle.Exit, L9_2)
  L5_2 = Event.Create
  L6_2 = Event.TimerRelative
  L7_2 = {}
  L7_2[1] = 1.5
  
  function L8_2(A0_3, A1_3, A2_3, A3_3)
    local L4_3, L5_3, L6_3, L7_3, L8_3, L9_3, L10_3, L11_3, L12_3, L13_3, L14_3, L15_3, L16_3, L17_3
    if A1_3 then
      L4_3 = Object.IsAlive(A1_3)
      if L4_3 then
        L4_3 = Object.GetHealth(A1_3)
        if 5 < L4_3 then
          L4_3 = Object.GetHardpointPosition
          L5_3 = A0_3
          L6_3 = "hp_veh_spawn"
          L4_3, L5_3, L6_3 = L4_3(L5_3, L6_3)
          L7_3 = Object.GetYaw(A0_3)
          L8_3 = {}
          L8_3[1] = 7.064171
          L8_3[2] = 0
          L8_3[3] = 1.165
          L9_3 = Pg.Spawn(A2_3, L4_3, L5_3, L6_3, L7_3, true, true, A1_3)
          Object.FadeOut(A1_3, 0.25, true)
          MrxPmc.AddCashQty(-tPrices[A0_3].nCost, nil)
          SettleIn(A0_3, L9_3, A3_3)
      end
    end
    else
      MrxSubtitle.Add("[buildings.repairBay.tooDamaged]")
      Object.OpenGate(A0_3)
      EndCinematic(A0_3, A3_3)
    end
  end
  
  L9_2 = {}
  L9_2[1] = A0_2
  L9_2[2] = A1_2
  L9_2[3] = L4_2
  L9_2[4] = A2_2
  L5_2(L6_2, L7_2, L8_2, L9_2)
end

Repair = L0_1

function L0_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = Event.Create
  L4_2 = Event.TimerRelative
  L5_2 = {}
  L5_2[1] = 2
  
  function L6_2(A0_3, A1_3)
    local L2_3, L3_3, L4_3, L5_3, L6_3, L7_3, L8_3, L9_3
    L2_3 = Player.GetCharacter(A1_3)
    L3_3 = Vehicle.GetDriver(A1_2)
    if L3_3 then
      Vehicle.Exit(A1_2, L3_3, true)
      Object.FadeOut(L3_3, 0.25, true)
    end
    bDriverIn = Vehicle.Enter(A1_2, L2_3, "d", true, false)
    L6_3 = {}
    L9_3 = tPrices[A0_3].nCost
    L6_3[1] = A1_2
    L6_3[2] = uVeh
    L6_3[3] = L9_3
    Event.Post("RepairBayUsed", L6_3)
    Object.OpenGate(A0_3)
    EndCinematic(A0_3, A1_3)
    MrxSubtitle.Add("[buildings.repairBay.repaired]")
  end
  
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = A2_2
  L3_2(L4_2, L5_2, L6_2, L7_2)
end

SettleIn = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2
  Player.SetCinematicMode(A1_2, false)
  Camera.SetYaw(Player.GetCamera(A1_2), 0)
end

EndCinematic = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  L7_2 = L3_2
  L4_2 = Pg.FastCollectGroundVehicles(L1_2, L2_2, L7_2, 15)
  Object.CloseGate(A0_2)
  L5_2 = ipairs
  L6_2 = L4_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = Object.IsAlive(L9_2)
    if not L10_2 then
      Object.FadeOut(L9_2, 1, true)
    end
  end
  L5_2 = tEvents[A0_2]
  L8_2 = {}
  L8_2[1] = Player.GetAnyCharacter()
  L8_2[2] = A0_2
  L8_2[3] = "<"
  L8_2[4] = 30
  L8_2[5] = true
  L8_2[6] = false
  L10_2 = {}
  L10_2[1] = A0_2
  L5_2.eDoorTriggerIn = Event.Create(Event.ObjectProximity, L8_2, PlayerInside, L10_2)
end

PlayerOutside = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  Object.OpenGate(A0_2)
  L1_2 = tEvents[A0_2]
  L4_2 = {}
  L4_2[1] = Player.GetAnyCharacter()
  L4_2[2] = A0_2
  L4_2[3] = ">"
  L4_2[4] = 30
  L4_2[5] = true
  L4_2[6] = false
  L6_2 = {}
  L6_2[1] = A0_2
  L1_2.eDoorTriggerOut = Event.Create(Event.ObjectProximity, L4_2, PlayerOutside, L6_2)
end

PlayerInside = L0_1
