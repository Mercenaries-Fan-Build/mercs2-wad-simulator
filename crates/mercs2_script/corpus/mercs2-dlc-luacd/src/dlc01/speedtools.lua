local L0_1, L1_1, L2_1
import("MrxTutorialManager", false)
import("DlcCon001", false)
import("MrxVoSequence", false)
import("MrxUtil", false)
import("MrxGuiBase", false)
uVehicle = nil
nMaxSpeed = 0
nMinSpeedAsPercent = 0
nReactorHealthSlot = 0
nSpeedTraySlot = 0
nLastZ = 0
nLastY = 0
nLastX = 0
nHeading = 0
nReactorHealth = 0
nBaseReactorHealth = 0
nHealthLoss = 0
bTimerActive = false
bInitialRed = false
bInitialYellow = false
bRedSparks = false
eEffect = nil
eMaterialAnimation = true

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  uVehicle = A0_2.uVehicle
  L3_2 = {}
  L3_2[1] = 0.1
  evSpeed = Event.CreatePersistent(Event.TimerRelative, L3_2, UpdateSpeed, {})
  L3_2 = {}
  L3_2[1] = uVehicle
  evVehicleDestroyed = Event.Create(Event.ObjectDeath, L3_2, VehicleExplosion, {})
  nMaxSpeed = A0_2.nMaxSpeed
  nMinSpeedAsPercent = A0_2.nMinSpeedAsPercent
  SetReactorHealth(A0_2.nReactorHealth)
  nBaseReactorHealth = A0_2.nReactorHealth
  nHealthLoss = A0_2.nHealthLoss
  nReactorHealthSlot = A0_2.nReactorHealthSlot
  nSpeedTraySlot = A0_2.nSpeedTraySlot
  DisplayReactorHealth()
  L1_2 = Event.Create
  L2_2 = Event.TimerRelative
  L3_2 = {}
  L3_2[1] = 15
  
  function L4_2()
    local L0_3, L1_3
    bTimerActive = true
  end
  
  L1_2(L2_2, L3_2, L4_2)
  L3_2 = {}
  L3_2[1] = 1
  evFlipped = Event.CreatePersistent(Event.TimerRelative, L3_2, Flipped, {})
end

InitializeSpeed = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  L0_2 = Vehicle.IsFlipped(uVehicle)
  if L0_2 then
    L0_2 = Object.GetVelocity(uVehicle)
    if L0_2 < 10 then
      L0_2 = {}
      L0_2[1] = "Fiona-In-Mission-Contract-Dlc01-13"
      L0_2[2] = "Fiona-In-Mission-Contract-Dlc01-14"
      tPossibleVO = L0_2
      sVOLine = MrxUtil.GetRandomTableElement(tPossibleVO)
      L0_2 = Event.Create
      L1_2 = Event.TimerRelative
      L2_2 = {}
      L2_2[1] = 1
      
      function L3_2()
        local L0_3, L1_3, L2_3
        L1_3 = {}
        L1_3[1] = sVOLine
        MrxVoSequence.Start(L1_3)
      end
      
      L0_2(L1_2, L2_2, L3_2)
    end
  end
end

Flipped = L0_1

function L0_1(A0_2)
  local L1_2
  nReactorHealth = A0_2
end

SetReactorHealth = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = Object.GetVelocity
  L2_2 = uVehicle
  L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2 = L1_2(L2_2)
  L0_2 = Math.abs(L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2)
  L1_2 = Object.GetPosition
  L2_2 = uVehicle
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  nHeading = Math.GetXZHeading((L1_2 - nLastX), (L2_2 - nLastY), (L3_2 - nLastZ))
  nLastZ = L3_2
  nLasty = L2_2
  nLastX = L1_2
  L4_2 = "green"
  L5_2 = nMaxSpeed * nMinSpeedAsPercent
  if L0_2 < L5_2 then
    L4_2 = "red"
  end
  L5_2 = Math.floor((L0_2 * 2.237))
  L6_2 = Hud.ObjectiveTray
  L8_2 = {}
  L8_2.vPlayer = nil
  L8_2.nSlot = nSpeedTraySlot
  L8_2.sText = ("[DLCCon001.UI.Speed] [" .. L4_2 .. "]" .. L5_2 .. " [DLCCon001.UI.MPH]")
  L6_2.SetSlotToText(L6_2, L8_2)
  L6_2 = Vehicle.IsFlipped(uVehicle)
  if L6_2 then
    L6_2 = Object.GetVelocity(uVehicle)
    if L6_2 < 10 then
      MrxTutorialManager.ShowMessage("[DLCCon001.UI.FlipCar]")
      L8_2 = {}
      L8_2[1] = 10
      Event.Create(Event.TimerRelative, L8_2, MrxTutorialManager.HideMessage, {})
    end
  end
  L6_2 = bTimerActive
  if L6_2 then
    L6_2 = nMaxSpeed * nMinSpeedAsPercent
    if L0_2 < L6_2 then
      L6_2 = evReactorTimer
      if not L6_2 then
        L8_2 = {}
        L8_2[1] = 1
        evReactorTimer = Event.CreatePersistent(Event.TimerRelative, L8_2, ReactorHealthLoss, {})
    end
    else
      L6_2 = nMaxSpeed * nMinSpeedAsPercent
      if L0_2 > L6_2 then
        L6_2 = evReactorTimer
        if L6_2 then
          ClearReactorTimer(true)
        end
      end
    end
  end
end

UpdateSpeed = L0_1

function L0_1()
  local L0_2, L1_2
  nReactorHealth = (nReactorHealth - nHealthLoss)
  DisplayReactorHealth()
  L0_2 = nReactorHealth
  if L0_2 <= 0 then
    Event.Delete(evVehicleDestroyed)
    VehicleExplosion()
    return
  end
end

ReactorHealthLoss = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  Event.Delete(evReactorTimer)
  evReactorTimer = nil
  if A0_2 then
    DisplayReactorHealth()
  end
end

ClearReactorTimer = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L1_2 = "white"
  if A0_2 then
    L1_2 = A0_2
  end
  L2_2 = "green"
  L3_2 = Math.ceil(((nReactorHealth / nBaseReactorHealth) * 100))
  if L3_2 < 40 then
    L2_2 = "red"
    L3_2 = bInitialRed
    if L3_2 then
      bInitialRed = false
      L3_2 = Object.GetPosition
      L4_2 = uVehicle
      L3_2, L4_2, L5_2 = L3_2(L4_2)
      eEffect = Pg.Spawn("dlc_global_particle_sparks_vehicle_box_red", L3_2, L4_2, L5_2, Object.GetYaw(uVehicle))
      Object.Attach(uVehicle, "bone_frame", eEffect)
      bRedSparks = true
      Sound.CueSound(uVehicle, "dlcfx_electric_lp-01")
      DlcCon001.PlayRandomVO("ReactorHealthLow")
      FlashText("red", true)
      return
    end
  else
    L3_2 = Math.ceil(((nReactorHealth / nBaseReactorHealth) * 100))
    if L3_2 < 70 then
      L2_2 = "yellow"
      bInitialRed = true
      L3_2 = bInitialYellow
      if L3_2 then
        bInitialYellow = false
        FlashText("yellow", true)
        return
      end
      L3_2 = bRedSparks
      if L3_2 then
        bRedSparks = false
        Object.Remove(eEffect)
        Sound.StopSound(uVehicle, "dlcfx_electric_lp-01")
      end
    end
  end
  if L2_2 == "green" then
    bInitialYellow = true
    bRedSparks = false
  end
  L3_2 = Hud.ObjectiveTray
  L5_2 = {}
  L5_2.nSlot = nReactorHealthSlot
  L5_2.sText = ("[" .. L1_2 .. "][DLCCon001.UI.ReactorStability] [" .. L2_2 .. "][bar" .. Math.ceil(((nReactorHealth / nBaseReactorHealth) * 100)) .. "]")
  L3_2.SetSlotToText(L3_2, L5_2)
end

DisplayReactorHealth = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  if A1_2 then
    Event.Delete(evFlash1)
    Event.Delete(evFlash2)
    Event.Delete(evFlash3)
  end
  DisplayReactorHealth(A0_2)
  L4_2 = {}
  L4_2[1] = 0.25
  evFlash1 = Event.Create(Event.TimerRelative, L4_2, DisplayReactorHealth, {})
  L4_2 = {}
  L4_2[1] = 0.5
  L6_2 = {}
  L6_2[1] = A0_2
  evFlash2 = Event.Create(Event.TimerRelative, L4_2, DisplayReactorHealth, L6_2)
  L4_2 = {}
  L4_2[1] = 0.75
  evFlash3 = Event.Create(Event.TimerRelative, L4_2, DisplayReactorHealth, {})
end

FlashText = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  bTimerActive = false
  Event.Delete(evSpeed)
  ClearReactorTimer(false)
  L0_2 = Object.GetPosition
  L1_2 = uVehicle
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  Pg.Spawn("Explosion (MOAB)", L0_2, L1_2, L2_2)
  Object.Kill(uVehicle)
end

VehicleExplosion = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = nHeading
  return L0_2
end

GetHeading = L0_1

function L0_1()
  local L0_2, L1_2
  bTimerActive = false
  Event.Delete(evSpeed)
  ClearReactorTimer(false)
end

DeinitSpeed = L0_1
tHealthPickupLocations = {}
tActiveHealthPickups = {}
sHealthPickupLabel = " "
nHealthPickupMinSpawnDist = 0
nHealthPickupMaxSpawnDist = 0
nHealthPickupSpawnRadius = 0
nHealthPickupOutofRange = 0
bHealthPacksEnabled = false
nSpawnChance = 0
nHealthRestored = 0
bHealthPickupsActive = false
bFirstHealthPickup = false

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  sHealthPickupLabel = A0_2.sHealthPickupLabel
  nHealthPickupMinSpawnDist = A0_2.nHealthPickupMinSpawnDist
  nHealthPickupMaxSpawnDist = A0_2.nHealthPickupMaxSpawnDist
  nHealthPickupSpawnRadius = A0_2.nHealthPickupSpawnRadius
  nHealthPickupOutofRange = A0_2.nHealthPickupOutofRange
  bHealthPacksEnabled = A0_2.bHealthPacksEnabled
  nSpawnChance = A0_2.nSpawnChance
  nHealthRestored = A0_2.nHealthRestored
  bHealthPickupsActive = true
  bFirstHealthPickup = true
  L1_2 = A0_2.nHealthPickups
  if L1_2 < 20 then
    SetupRandomHealthPickups(A0_2.nHealthPickups)
  else
    L1_2 = math.floor((A0_2.nHealthPickups / 20))
    SetupRandomHealthPickups(20)
    if 1 < L1_2 then
      L2_2 = 1
      L3_2 = L1_2 - 1
      L4_2 = 1
      for L5_2 = L2_2, L3_2, L4_2 do
        L8_2 = {}
        L8_2[1] = (30 * L5_2)
        L10_2 = {}
        L10_2[1] = 20
        Event.Create(Event.TimerRelative, L8_2, SetupRandomHealthPickups, L10_2)
      end
    end
    L2_2 = A0_2.nHealthPickups - (L1_2 * 20)
    if 0 < L2_2 then
      L4_2 = {}
      L4_2[1] = (30 * L1_2)
      L6_2 = {}
      L6_2[1] = (A0_2.nHealthPickups - (L1_2 * 20))
      Event.Create(Event.TimerRelative, L4_2, SetupRandomHealthPickups, L6_2)
    end
  end
end

InitializeRandomHealthPickups = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = 0
  L2_2 = 1
  L3_2 = A0_2
  L4_2 = 1
  for L5_2 = L2_2, L3_2, L4_2 do
    L1_2 = L1_2 + SelectRandomHealthPickup()
  end
  if 0 < L1_2 then
    L4_2 = {}
    L4_2[1] = 0.5
    L6_2 = {}
    L6_2[1] = L1_2
    evReselectHealthPickups = Event.Create(Event.TimerRelative, L4_2, SetupRandomHealthPickups, L6_2)
  end
end

SetupRandomHealthPickups = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2, L14_2, L15_2, L16_2, L17_2, L18_2, L19_2
  L0_2 = bHealthPickupsActive
  if not L0_2 then
    L0_2 = 0
    return L0_2
  end
  L0_2 = Object.GetPosition
  L1_2 = uVehicle
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  tHealthPickupLocations = Pg.GetObjectsInArea(L0_2, L1_2, L2_2, nHealthPickupMaxSpawnDist, sHealthPickupLabel)
  L3_2 = table.getn(tHealthPickupLocations)
  L4_2 = 1
  if 0 < L3_2 then
    L5_2 = tHealthPickupLocations[Math.randi(1, L3_2)]
    L6_2 = Object.GetPosition
    L7_2 = L5_2
    L6_2, L7_2, L8_2 = L6_2(L7_2)
    nHealthPickupHeading = Math.GetXZHeading((L6_2 - L0_2), (L7_2 - L1_2), (L8_2 - L2_2))
    L9_2 = Math.abs((nHealthPickupHeading - GetHeading()))
    L10_2 = nHealthPickupSpawnRadius
    if not (L9_2 > L10_2) then
      L9_2 = Object.GetDistanceFrom(uVehicle, L5_2)
      L10_2 = nHealthPickupMinSpawnDist
      if not (L9_2 < L10_2) then
        goto lbl_65
      end
    end
    L9_2 = 1
    do return L9_2 end
    ::lbl_65::
    L9_2 = pairs
    L10_2 = tActiveHealthPickups
    L9_2, L10_2, L11_2 = L9_2(L10_2)
    for L12_2, L13_2 in L9_2, L10_2, L11_2 do
      L14_2 = L13_2.uLocationGuid
      if L14_2 == L5_2 then
        L14_2 = 1
        return L14_2
      end
    end
    L11_2 = {}
    L11_2.uLocationGuid = L5_2
    table.insert(tActiveHealthPickups, L11_2)
    L9_2 = 0
    L10_2 = pairs
    L11_2 = tActiveHealthPickups
    L10_2, L11_2, L12_2 = L10_2(L11_2)
    for L13_2, L14_2 in L10_2, L11_2, L12_2 do
      L15_2 = L14_2.uLocationGuid
      if L15_2 == L5_2 then
        L9_2 = L13_2
      end
    end
    L10_2 = Math.randi(1, nSpawnChance) == 1
    L11_2 = bHealthPacksEnabled
    if L11_2 and L10_2 then
      L11_2 = Pg.Spawn("DLC_Speed_Boost_Pickup", L6_2, L7_2, L8_2)
      Object.DisablePhysics(L11_2)
      L12_2 = tActiveHealthPickups[L9_2]
      L12_2.uPickupGuid = L11_2
      L12_2 = tActiveHealthPickups[L9_2]
      L15_2 = {}
      L15_2[1] = uVehicle
      L15_2[2] = L11_2
      L15_2[3] = "<"
      L15_2[4] = 5
      L17_2 = {}
      L17_2[1] = L11_2
      L12_2.evPickup = Event.Create(Event.ObjectProximity, L15_2, PickupHealth, L17_2)
    end
    L11_2 = tActiveHealthPickups[L9_2]
    L14_2 = {}
    L14_2[1] = uVehicle
    L14_2[2] = L5_2
    L14_2[3] = ">"
    L14_2[4] = nHealthPickupOutofRange
    L16_2 = {}
    L16_2[1] = L5_2
    L11_2.evHealthPickupOutofRange = Event.Create(Event.ObjectProximity, L14_2, RemoveHealthPickup, L16_2)
    L11_2 = 0
    return L11_2
  else
    L5_2 = 1
    return L5_2
  end
end

SelectRandomHealthPickup = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = bFirstHealthPickup
  if L1_2 then
    bFirstHealthPickup = false
    L2_2 = {}
    L2_2[1] = "Fiona-In-Mission-Contract-Dlc01-07"
    L2_2[2] = ShowHealthTutorial
    MrxVoSequence.Start(L2_2)
  end
  Object.Remove(A0_2)
  L3_2 = "dlcfx_nuke_health"
  Sound.CueSound(uVehicle, L3_2)
  L1_2 = pairs
  L2_2 = tActiveHealthPickups
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.uPickupGuid
    if L6_2 == A0_2 then
      Event.Delete(L5_2.evHealthPickupOutofRange)
    end
  end
  nReactorHealth = (nReactorHealth + nHealthRestored)
  L1_2 = nReactorHealth
  L2_2 = nBaseReactorHealth
  if L1_2 > L2_2 then
    nReactorHealth = nBaseReactorHealth
  end
  L1_2 = Object.GetPosition
  L2_2 = uVehicle
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  Object.Attach(uVehicle, "bone_frame", Pg.Spawn("dlc_global_particle_sparks_vehicle_box_blue", L1_2, L2_2, L3_2, Object.GetYaw(uVehicle)))
  L6_2 = eMaterialAnimation
  if L6_2 then
    eMaterialAnimation = false
    Object.PlayMaterialAnimation(uVehicle, "energy_wave", false)
    L8_2 = {}
    L8_2[1] = 1
    Event.Create(Event.TimerRelative, L8_2, _GraphicsAto, {})
  end
  FlashText("green", true)
end

PickupHealth = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  MrxTutorialManager.ShowMessage("[DLCCon001.UI.HealthKit]")
  L2_2 = {}
  L2_2[1] = 10
  Event.Create(Event.TimerRelative, L2_2, MrxTutorialManager.HideMessage, {})
end

ShowHealthTutorial = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2
  Object.PlayMaterialAnimation(uVehicle, "energy_coil", true)
  eMaterialAnimation = true
end

_GraphicsAto = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L1_2 = pairs
  L2_2 = tActiveHealthPickups
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  for L4_2, L5_2 in L1_2, L2_2, L3_2 do
    L6_2 = L5_2.uLocationGuid
    if L6_2 == A0_2 then
      L6_2 = L5_2.uPickupGuid
      if L6_2 then
        L6_2 = Object.IsAlive(L5_2.uPickupGuid)
        if L6_2 then
          Object.Remove(L5_2.uPickupGuid)
        end
      end
      Event.Delete(L5_2.evPickup)
      table.remove(tActiveHealthPickups, L4_2)
    end
  end
  SetupRandomHealthPickups(1)
end

RemoveHealthPickup = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  bHealthPickupsActive = false
  Event.Delete(evReselectHealthPickups)
  L0_2 = pairs
  L1_2 = tActiveHealthPickups
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    L5_2 = L4_2.uPickupGuid
    if L5_2 then
      L5_2 = Object.IsAlive(L4_2.uPickupGuid)
      if L5_2 then
        Object.Remove(L4_2.uPickupGuid)
      end
    end
    Event.Delete(L4_2.evPickup)
    Event.Delete(L4_2.evHealthPickupOutofRange)
  end
  tActiveHealthPickups = {}
end

DeinitRandomHealthPickups = L0_1
nRumbleLength = 0
nRumbleIntensity = 0
nBoostUseRate = 0
bIsBoosting = false

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  bIsBoosting = false
  nRumbleLength = A0_2.nRumbleLength
  nRumbleIntensity = A0_2.nRumbleIntensity
  L1_2 = A0_2.nBoostUseRate
  if L1_2 then
    nBoostUseRate = (A0_2.nBoostUseRate / 4)
  end
  L1_2 = A0_2.bEnableBoost
  if L1_2 then
    L3_2 = {}
    L3_2[1] = Player.GetLocalPlayer()
    L3_2[2] = "lbutton"
    L3_2[3] = "press"
    L3_2[4] = true
    evTriggerBoost = Event.CreatePersistent(Event.Button, L3_2, TriggerBoost, {})
  else
    Event.Delete(evTriggerBoost)
    Event.Delete(evStopBoost)
    Event.Delete(evApplyBoost)
  end
end

SetupBoost = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = Player.GetLocalPlayer
  L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2 = L1_2()
  L0_2 = MrxGuiBase.GetCurrentControlHolder(L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2)
  if L0_2 then
    return
  end
  L0_2 = nReactorHealth
  L1_2 = nBoostUseRate
  if L0_2 < L1_2 then
    return
  end
  bIsBoosting = true
  L0_2 = 3000
  L1_2 = Object.GetVelocity(uVehicle)
  if L1_2 < 15 then
    L0_2 = 50000
  end
  L3_2 = {}
  L3_2[1] = Player.GetLocalPlayer()
  L3_2[2] = "lbutton"
  L3_2[3] = "release"
  L3_2[4] = true
  evStopBoost = Event.CreatePersistent(Event.Button, L3_2, StopBoost, {})
  L1_2 = String.GetHash("DLC_global_particle_fire_jetengine_boost_infinite")
  nReactorHealth = (nReactorHealth - nBoostUseRate)
  DisplayReactorHealth()
  ObjectState.StartEmitter(uVehicle, String.GetHash("hp_fx_exhaust_a"), L1_2)
  ObjectState.StartEmitter(uVehicle, String.GetHash("hp_fx_exhaust_b"), L1_2)
  Pg.Rumble(Player.GetLocalPlayer(), nRumbleLength, nRumbleIntensity)
  Object.ApplyImpulse(uVehicle, 0, 0, L0_2, true)
  L4_2 = {}
  L4_2[1] = 0.25
  evApplyBoost = Event.CreatePersistent(Event.TimerRelative, L4_2, ApplyBoost, {})
end

TriggerBoost = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L0_2 = nReactorHealth
  L1_2 = nBoostUseRate
  if L0_2 < L1_2 then
    StopBoost()
    return
  end
  L0_2 = 3000
  L1_2 = Object.GetVelocity(uVehicle)
  if L1_2 < 10 then
    L0_2 = 6000
  end
  Object.ApplyImpulse(uVehicle, 0, 0, L0_2, true)
  L1_2 = Vehicle.IsFlipped(uVehicle)
  if L1_2 then
    L1_2 = Object.GetVelocity(uVehicle)
    if L1_2 < 10 then
      L1_2 = Object.GetMass(uVehicle)
      Object.ApplyAngularImpulse(uVehicle, (-L1_2 * 2.5), 0, (L1_2 * 2.5), true)
    end
  end
  nReactorHealth = (nReactorHealth - nBoostUseRate)
  DisplayReactorHealth()
end

ApplyBoost = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2
  bIsBoosting = false
  L0_2 = String.GetHash("DLC_global_particle_fire_jetengine_boost_infinite")
  ObjectState.StopEmitter(uVehicle, String.GetHash("hp_fx_exhaust_a"), L0_2)
  ObjectState.StopEmitter(uVehicle, String.GetHash("hp_fx_exhaust_b"), L0_2)
  Event.Delete(evApplyBoost)
end

StopBoost = L0_1

function L0_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2
  L0_2 = "green"
  L1_2 = nBoostAvailable
  if L1_2 < 66 then
    L0_2 = "yellow"
  end
  L1_2 = nBoostAvailable
  if L1_2 < 33 then
    L0_2 = "red"
  end
  L1_2 = Hud.ObjectiveTray
  L3_2 = {}
  L3_2.nSlot = nBoostTraySlot
  L3_2.sText = ("Boost [" .. L0_2 .. "][bar" .. nBoostAvailable .. "]")
  L1_2.SetSlotToText(L1_2, L3_2)
end

DisplayBoost = L0_1

function L0_1()
  local L0_2, L1_2
  L0_2 = bIsBoosting
  return L0_2
end

IsBoosting = L0_1
