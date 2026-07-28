import("MrxGui")
import("MrxSound")
import("MrxTutorialManager")
import("MrxUtil")
import("MrxVoSequence")
import("MrxPmc")
import("WifEquipmentData")
import("MrxParkingLotManager")
tEvent = {}
local nInitialDelay = 7.5
local nSurvivalDelay = 4
local nHealingFactor = 0.03
local nVehicleFactor = 2
local nTic = 2.5
local nMinTic = 0.3
local bSurvivalMode = {}
local bSeeingRed = {}
local nSurvivalThreshold = 10
local nSurvivalCooldown = 3
local nSurvivalAlpha = 100
local nMinTimeScale = 1
local nMaxTimeScale = 0.45

function Init()
  MrxParkingLotManager.Setup()
end

function Deinit()
  MrxParkingLotManager.Cleanup()
end

function OnActivate(uGuid)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Activate, {uGuid})
end

function Activate(uGuid)
  local uPlayer = Object.IsPlayerControlled(uGuid)
  if uPlayer then
    if Player.IsLocal(uPlayer) then
      if GetAttribute(uGuid, "attitude") > 2 then
        nHealingFactor = nHealingFactor * 1.5
        nSurvivalCooldown = nSurvivalCooldown * 1.5
      end
      tEvent[uGuid] = tEvent[uGuid] or {}
      SetupSurvivalSystem(uPlayer, uGuid, true)
      SetupInventory(uGuid)
      SetupTransferSystem(uPlayer, uGuid)
    end
  else
    Debug.Printf("WARNING: Trying to set up Survival mode for a non-player ( " .. tostring(Object.GetName(uGuid)) .. " )")
  end
  Event.Create(Event.TimerRelative, {3}, function()
    Gui.SetPickupMarkerVisibleDistance(20, false)
    Gui.SetPickupMarkerSize(18, false)
    Gui.SetPickupMarkerSize(18, true)
  end)
end

function OnDeath(uGuid)
  local uPlayer = Object.IsPlayerControlled(uGuid)
  if uPlayer then
    if not Player.IsLocal(uPlayer) then
      return
    end
  else
    Debug.Printf("WARNING: calling hero.OnDeath for a non-player ( " .. tostring(Object.GetName(uGuid)) .. " )")
    return
  end
  CleanupSurvival(uGuid)
  MrxSound.EndSurvivalMode()
  tEvent[uGuid].FullHealth = Event.Create(Event.ObjectHealth, {
    uGuid,
    ">",
    5
  }, SetupSurvivalSystem, {
    uPlayer,
    uGuid,
    true
  })
end

function OnDeactivate(uGuid)
  local uPlayer = Object.IsPlayerControlled(uGuid)
  if not uPlayer then
    return
  end
  if not Player.IsLocal(uPlayer) then
    return
  end
  SaveOutInventory(uGuid)
  CleanupSurvival(uGuid)
  CleanupTransferSystem(uPlayer, uGuid)
end

function SetupInventory(uGuid)
  local tWeapons = Human.Inventory.GetAllWeapons(uGuid)
  for i, weapon in pairs(tWeapons) do
    local max = Weapon.GetMaxReserveAmmo(weapon)
    if max then
      Weapon.SetReserveAmmo(weapon, max)
    end
  end
end

function SaveOutInventory(uGuid)
  local tWeapons = Human.Inventory.GetAllWeapons(uGuid)
end

function HealthDropped(uGuid)
  Event.Delete(tEvent[uGuid].Timer)
  tEvent[uGuid].Timer = nil
  local uPlayer = Object.IsPlayerControlled(uGuid)
  local nHealth = Object.GetHealth(uGuid)
  if bSurvivalMode[uPlayer] then
    CreateHealTimer(uGuid, nSurvivalDelay)
  elseif nHealth < nSurvivalThreshold and not tEvent[uGuid].Cooldown then
    StartSurvivalMode(uPlayer, uGuid)
  elseif tEvent[uGuid].Cooldown then
  else
    CreateHealTimer(uGuid, nInitialDelay)
  end
  CreateDropEvent(uGuid)
end

function Heal(uGuid)
  local nCurrentHealth = Object.GetHealth(uGuid)
  local nMaxHealth = Object.GetMaxHealth(uGuid)
  if not nCurrentHealth then
    return
  end
  local nHeal = (nMaxHealth - nCurrentHealth) * nHealingFactor
  if Object.InSeat(uGuid) then
    nHeal = nHeal * nVehicleFactor
  end
  local nNextPulse = Math.max(nTic / nHeal, nMinTic)
  nHeal = Math.max(nHeal, 1)
  Object.SetHealth(uGuid, Math.min(nCurrentHealth + nHeal, nMaxHealth))
  if nCurrentHealth < nMaxHealth then
    CreateHealTimer(uGuid, nNextPulse)
  end
  local uPlayer = Object.IsPlayerControlled(uGuid)
  if bSurvivalMode[uPlayer] and nCurrentHealth >= nSurvivalThreshold then
    EndSurvivalMode(uPlayer, uGuid)
  end
  CreateDropEvent(uGuid)
end

function CreateHealTimer(uGuid, nNextPulse)
  Event.Delete(tEvent[uGuid].Timer)
  tEvent[uGuid].Timer = nil
  tEvent[uGuid].Timer = Event.Create(Event.TimerRelative, {nNextPulse}, Heal, {uGuid})
  local uPlayer = Object.IsPlayerControlled(uGuid)
end

function CreateDropEvent(uGuid)
  Event.Delete(tEvent[uGuid].HealthDropped)
  tEvent[uGuid].HealthDropped = nil
  tEvent[uGuid].HealthDropped = Event.CreatePersistent(Event.ObjectHealth, {
    uGuid,
    "<",
    Object.GetHealth(uGuid)
  }, HealthDropped, {uGuid})
end

function CleanupSurvival(uGuid)
  local uPlayer = Object.IsPlayerControlled(uGuid)
  if not uPlayer then
    CleanEvents(tEvent[uGuid])
    return
  end
  if tEvent[uGuid] and tEvent[uGuid].Cooldown then
    SurvivalCooldownEnded(uPlayer, uGuid)
  end
  Debug.Printf("BLACKSCREEN - FadeFromColor called from hero.CleanupSurvival")
  MrxGui.FadeFromColor(0, uPlayer)
  CleanEvents(tEvent[uGuid])
end

function CleanEvents(events)
  if type(events) ~= "table" then
    return
  end
  for _, event in pairs(events) do
    if type(event) == "table" then
      CleanEvents(event)
    else
      Event.Delete(event)
    end
  end
end

function SetupSurvivalSystem(uPlayer, uGuid, bKickStartEvents)
  bSurvivalMode[uPlayer] = false
  Player.SetSurvivalMode(uPlayer, false)
  Player.SetHealthClamp(uPlayer, true)
  if bKickStartEvents then
    HealthDropped(uGuid)
  end
end

function SurvivalModeCallback(uPlayer, uGuid, bCallback)
end

function EndSurvivalMode(uPlayer, uGuid, nTime)
  SetupSurvivalSystem(uPlayer, uGuid)
  if "number" ~= type(nTime) then
    nTime = 2
  end
  Debug.Printf("BLACKSCREEN - FadeFromColor called from hero.EndSurvivalMode")
  MrxGui.FadeFromColor(nTime, uPlayer)
  MrxSound.EndSurvivalMode()
end

function StartSurvivalMode(uPlayer, uGuid, bCallback)
  local sTail = " --> Called from HealthDropped"
  if bCallback then
    sTail = " --> Callback function"
  end
  if tEvent[uGuid] and tEvent[uGuid].Cooldown or bSurvivalMode[uPlayer] then
    return
  end
  Event.Post("SurvivalMode", {uGuid})
  uGuid = uGuid or Player.GetCharacter(uPlayer)
  Debug.Printf("BLACKSCREEN - FadeToColor called from hero.StartSurvivalMode")
  MrxGui.FadeToColor(0.3, uPlayer, 255, 0, 0, nSurvivalAlpha)
  Object.SetInvincible(uGuid, true, "Survival")
  tEvent[uGuid].Cooldown = Event.CreatePersistent(Event.TimerRelative, {nSurvivalCooldown}, SurvivalCooldownEnded, {uPlayer, uGuid})
  MrxSound.BeginSurvivalMode()
end

function SurvivalCooldownEnded(uPlayer, uGuid)
  Event.Delete(tEvent[uGuid].Cooldown)
  tEvent[uGuid].Cooldown = nil
  bSurvivalMode[uPlayer] = true
  Object.SetInvincible(uGuid, false, "Survival")
  Player.SetSurvivalMode(uPlayer, true)
  Player.SetHealthClamp(uPlayer, false)
  Debug.Printf("BLACKSCREEN - FadeFromColor called from hero.SurvivalCooldownEnded")
  MrxGui.FadeFromColor(3, uPlayer)
  Event.Post("SurvivalCooldownEnded", {uGuid})
  CreateHealTimer(uGuid, nSurvivalDelay)
  tEvent[uGuid].nTimeScale = nMaxTimeScale
end

function SetTimeScale(uPlayer, uGuid)
  do return end
  local nCurrentHealth = Object.GetHealth(uGuid)
  local nNewTimeScale = nCurrentHealth / nSurvivalThreshold * (1 - nMaxTimeScale) + nMaxTimeScale
  Sys.SetTimeScale(Math.min(nNewTimeScale, nMinTimeScale))
end

function SetupTransferSystem(uGuid)
  Debug.Printf("+++++++++++++++++++++++++++ Transfer Activate")
  nRiders = nRiders or 0
  nRiders = nRiders + 1
  if uRiderEvent then
    Debug.Printf("+++++++++++++++++++++++++++ Transfer Already Active")
    return
  end
  uRiderEvent = Event.CreatePersistent(Event.ObjectInSeat, {
    Player.GetLocalCharacter(),
    0,
    "pg",
    "e"
  }, EnterPassengerCallback)
end

function CleanupTransferSystem(uGuid)
  if not nRiders or not uRiderEvent then
    return
  end
  Debug.Printf("+++++++++++++++++++++++++++ Transfer Cleanup")
  nRiders = nRiders - 1
  if nRiders == 0 and uRiderEvent then
    Event.Delete(uRiderEvent)
    uRiderEvent = nil
    nRiders = nil
  end
end

function EnterPassengerCallback(uGuid, uVehicle, sSeatType, uSeat)
  local uDriver = Vehicle.GetDriver(uVehicle)
  if uDriver == uGuid then
    return
  end
  if Vehicle.IsSeatALadder(uSeat) then
    return
  end
  if uDriver then
    if Object.IsPlayerControlled(uDriver) then
      Debug.Printf("Transfer aborted - Driver is a player")
      return
    end
    if not Object.HasLabel(uVehicle, "Car") and not Object.HasLabel(uVehicle, "Truck") then
      Debug.Printf("Transfer failed - Unsupported vehicle type")
      return
    end
    if sSeatType == "gunner" and Ai.GetRelation(uGuid, uDriver) >= 0 then
      Debug.Printf("Transfer aborted - In Gunner seat and driver is friendly")
      return
    end
  elseif sSeatType == "gunner" then
    Debug.Printf("Transfer aborted - In gunner seat w/o driver")
    return
  end
  local uDriverSeat = Vehicle.GetSeatByType(uVehicle, "d")
  if not uDriverSeat or Vehicle.IsSeatBlocked(uDriverSeat) then
    Debug.Printf("Transfer failed - No driver seat or drive seat is blocked")
    return
  end
  local uSeat = Vehicle.GetSeatFromRider(uGuid)
  local tTransfers = Vehicle.GetSeatToSeat(uSeat, false)
  local bTransfer = false
  if tTransfers then
    for _, uTransferSeat in ipairs(tTransfers) do
      if uTransferSeat == uDriverSeat then
        bTransfer = true
        break
      end
    end
  end
  if not bTransfer then
    Debug.Printf("Transfer failed - No Link between driver seat and current seat")
    return
  end
  if uDriver then
    Vehicle.Exit(uVehicle, uDriver, false)
    Event.Create(Event.ObjectInSeat, {
      uDriver,
      0,
      "a",
      "x"
    }, Vehicle.TransferToSeat, {
      uGuid,
      uDriverSeat,
      false
    })
  else
    Vehicle.TransferToSeat(uGuid, uDriverSeat, false)
  end
end

function GetAttribute(uGuid, sAttribute)
  if not uGuid or not sAttribute then
    return
  end
  uGuid = Player.GetCharacter(uGuid) or uGuid
  if not Object.IsPlayerControlled(uGuid) then
    Debug.Printf("ERROR: Passed in guid was neither a player nor a character!")
    return
  end
  local nLevel
  for i = 1, 3 do
    if Object.HasLabel(uGuid, tostring(sAttribute .. i)) then
      nLevel = i
    end
    i = i + 1
  end
  if not nLevel then
    Debug.Printf("ERROR: No matching attribute found. Using default value of 2")
    nLevel = 2
  end
  return nLevel
end

_uHideMessage = nil

function HideTutorialMessage()
  if type(_uHideMessage) == "userdata" then
    MrxTutorialManager.HideMessage(true)
    Event.Delete(_uHideMessage)
  end
  _uHideMessage = nil
end

function TutorialCueCallback()
  _uHideMessage = Event.Create(Event.TimerRelative, {5}, HideTutorialMessage, {})
end

function DisableGrappleTriggered(uPlayerGuid)
  if not _uHideMessage and not MrxPmc.HasEquipment("GrapplingHook") then
    local sTutorialString = "[Tutorial.Grapple.Key1]"
    if MrxTutorialManager.ShowMessage(sTutorialString, true) then
      local id = MrxUtil.GetCharacterIdentity(uPlayerGuid)
      CueTable = {
        mattias = "Mattias.Grapple01",
        jennifer = "Jen.Grapple",
        chris = "Chris.Grapple01"
      }
      MrxVoSequence.Start({
        {
          CueTable[id],
          uPlayerGuid
        },
        TutorialCueCallback
      }, nil, MrxVoSequence.knPriorityFreeplay)
      _uHideMessage = "InValid"
    end
  end
end
