import("MrxSupportDesignator")
import("MrxSupportManager")
import("MrxGui")
import("MrxPmc")
import("MrxGuiHudMessage")
import("AntiAir")
import("MrxAchievements")
import("MrxUtil")
import("MrxVoSequence")
import("MrxFactionManager")
oDesignator = nil
sDeliveryVehicle = "Support Vehicle (Mig27)"
uDeliveryVehicle = Pg.GetGuidByName("Support Vehicle (Mig27)")
sBomb = "Dumb Bomb Projectile"
uBomb = Pg.GetGuidByName("Dumb Bomb Projectile")
uOwner = nil
nAircraftBlip = 0
tEvents = {}
tAA = {}
tAA.basic = 0
tAA.medium = 0
tAA.advanced = 0
tAA.jammer = 0
sRecruit = "Arachnid Guy"
tVOCues = {}
tLocalNetObjects = {}
tRemoteNetObjects = {}

function Create(self, uPlayerGuid)
  local oNewSupport = o or {}
  setmetatable(oNewSupport, self)
  self.__index = self
  oNewSupport:SetDesignator(nil)
  oNewSupport.sDeliveryVehicle = self.sDeliveryVehicle
  oNewSupport.uDeliveryVehicle = Pg.GetGuidByName(self.sDeliveryVehicle)
  oNewSupport.sBomb = self.sBomb
  oNewSupport.uBomb = Pg.GetGuidByName(self.sBomb)
  oNewSupport:SetOwner(uPlayerGuid or self.uOwner)
  oNewSupport:SetModuleName("MrxSupport")
  return oNewSupport
end

function DesignationCallback(self)
end

function GetDesignator(self)
  return self.oDesignator
end

function SetDesignator(self, oDesignator)
  self.oDesignator = oDesignator
  if oDesignator then
    oDesignator:SetParentSupport(self)
  end
end

function SetModuleName(self, sModuleName)
  self.sModuleName = sModuleName
end

function GetModuleName(self)
  return self.sModuleName
end

function GetModule(self)
  return ObjectState.GetStringHash(self.sModuleName)
end

function SetOwner(self, uGuid)
  if "userdata" ~= type(uGuid) then
    return
  end
  self.uOwner = uGuid
  if self.oDesignator then
    self.oDesignator:SetOwner(uGuid)
  end
end

function SetFaction(self, sFactionId)
  self.sFactionId = sFactionId
end

function GetFaction(self)
  return self.sFactionId
end

function GetDenialCondition(self)
  if self.oDesignator and self.oDesignator.sAATestLevel then
    local sError = "[PDA.Support.denied." .. tostring(self.oDesignator.sAATestLevel) .. "]"
    if TestAALevel(self.oDesignator.sAATestLevel) then
      return sError
    end
  end
  if self:GetFaction() and MrxFactionManager.GetAttitudeLabel(self:GetFaction(), "Pmc") == "Hostile" then
    return "[Generic.Attitudes.Hostile]"
  end
  if not MrxSupportManager.IsRecruitAvailable(self:GetRecruit()) then
    return "[PDA.Support.denied.rearming]"
  end
  return nil
end

function GetOwner(self, uGuid)
  return self.uOwner
end

function SetRecruit(self, sRecruit)
  self.sRecruit = sRecruit
end

function GetRecruit(self)
  return self.sRecruit
end

function GetElapsedCooldownTime(self)
  if "[PDA.Support.denied.rearming]" == self:GetDenialCondition() then
    return MrxSupportManager.GetRecruitTimes(self:GetRecruit())
  end
end

function SetSupportName(self, sSupportName)
  self.sSupportName = sSupportName
end

function GetSupportName(self)
  return self.sSupportName
end

function SetFuelCost(self, nFuelCost)
  self.nFuelCost = nFuelCost
end

function GetFuelCost(self)
  return self.nFuelCost or 0
end

function SetCashCost(self, nCashCost)
  self.nCashCost = nCashCost
end

function GetCashCost(self)
  return self.nCashCost
end

function ShouldSuppressIconAnimationOnDirectUse(self)
  if self.oDesignator and self.oDesignator.ShouldSuppressIconAnimationOnDirectUse then
    return self.oDesignator:ShouldSuppressIconAnimationOnDirectUse()
  end
  return true
end

function SetVOCues(self, tVOCues)
  self.tVOCues = tVOCues
end

function GetVOCues(self)
  return self.tVOCues
end

function PlayRandomVOCue(tVOCues, bSendNetEvent)
  if type(tVOCues) == "table" then
    bSendNetEvent = MrxUtil.SetDefault(bSendNetEvent, true)
    local sCue = MrxUtil.GetRandomTableElement(tVOCues)
    MrxVoSequence.Start(sCue, nil, MrxVoSequence.knPriorityFreeplay, bSendNetEvent)
  else
    Debug.Printf("PlayRandomVOCue was called with no list of cues!")
  end
end

function BeginSupportSequence(self)
  Debug.Printf(self)
  local sSupportName = self:GetSupportName()
  local nFuelCost = self:GetFuelCost()
  if nFuelCost then
    local nFuelUsed = math.min(nFuelCost, MrxPmc.GetFuelQty())
    MrxPmc.AddFuelQty(nFuelUsed * -1)
    self.nFuelConsumed = nFuelUsed
  end
  local nFreeCount = MrxPmc.GetFreebieQty(sSupportName)
  if nFreeCount then
    if nFreeCount < 1 then
      local nCashCost = self:GetCashCost()
      if nCashCost then
        MrxPmc.AddCashQty(-nCashCost)
      end
    else
      MrxPmc.AddFreebieQty(sSupportName, -1)
      self.sFreebieConsumed = sSupportName
    end
  else
    local nStockCount = MrxPmc.GetSupportQty(sSupportName)
    if nStockCount then
      MrxPmc.AddSupportQty(sSupportName, -1)
      self.sStockpileConsumed = sSupportName
    end
  end
  if Net.SendEvent_Support then
    local nX, nY, nZ, uDesignator, uTarget, bEventPost
    if self.oDesignator then
      nX, nY, nZ, uDesignator, uTarget = self.oDesignator:GetTarget()
    end
    local finalDestination = self.oFinalDestination
    if "string" == type(finalDestination) then
      finalDestination = Pg.GetGuidByName(finalDestination)
    end
    local setBomb = self.sBomb
    if "string" == type(setBomb) then
      setBomb = Pg.GetGuidByName(setBomb)
    end
    if self.oDesignator.bDesignationComplete then
      bEventPost = true
    end
    Net.SendEvent_Support(self:GetModule(), nX or 0, nY or 0, nZ or 0, uDesignator or 0, uTarget or 0, self.uCargoToDeliver or 0, finalDestination, self.uDeliveryVehicle, setBomb, bEventPost)
  end
  Event.Post("SupportUsed", self)
  if self:GetRecruit() == "Copter" then
    MrxSupportManager.StartRecruitCooldown("Copter", 60)
  end
  return self:DesignationCallback()
end

function Configure(self, tOptions)
  for Index in pairs(tOptions) do
    self[Index] = tOptions[Index] or self[Index]
  end
end

function Commence(self, bFireImmediately)
  if "table" ~= type(self.oDesignator) then
    if self.uOwner then
      MrxSupportManager.CurrentlyEquippedSupport:AddSupport(self)
    end
    self:BeginSupportSequence()
    return true
  end
  if "function" ~= type(self.oDesignator.GetTarget) then
    return false
  end
  if "function" ~= type(self.oDesignator.Configure) then
    return false
  end
  if "function" ~= type(self.oDesignator.Commence) then
    return false
  end
  if not MrxSupportManager.IsRecruitAvailable(self:GetRecruit()) then
    return false
  end
  self.oDesignator:SetOwner(self.uOwner)
  self.oDesignator:SetCompleteCallback(BeginSupportSequence, {self})
  if self.uOwner then
    MrxSupportManager.CurrentlyEquippedSupport:AddSupport(self)
  end
  return self.oDesignator:Commence(bFireImmediately)
end

function BlipAircraft(uAircraft, tColor, bSticky, sTexture)
  bSticky = bSticky or false
  local tColor = tColor or {
    0,
    173,
    239
  }
  nAircraftBlip = nAircraftBlip + 1
  local sBlipName = tostring(uAircraft)
  Hud.Radar:AddObjective({
    sName = sBlipName,
    nR = tColor[1],
    nG = tColor[2],
    nB = tColor[3],
    nWidth = 3,
    nHeight = 3,
    sTexture = sTexture,
    uGuid = uAircraft,
    bSticky = bSticky,
    nSortOrder = 4
  })
  tEvents[sBlipName] = {}
  tEvents[sBlipName].uGuid = uAircraft
  tEvents[sBlipName].uHibernation = Event.Create(Event.ObjectHibernation, {uAircraft, "hibernated"}, _RemoveBlipCallback, {sBlipName, true})
  tEvents[sBlipName].uDeath = Event.Create(Event.ObjectDeath, {uAircraft}, _RemoveBlipCallback, {sBlipName, false})
  local uDriver = Vehicle.GetDriver(uAircraft)
  if uDriver then
  end
  return sBlipName
end

function _RemoveBlipCallback(sBlipName, bDelete)
  Hud.Radar:RemoveObjective({sName = sBlipName})
  if Net.IsServer() then
    Net.SendEvent_RemoveObjective(sBlipName)
  end
  if tEvents[sBlipName] then
    if bDelete and Object.IsAlive(tEvents[sBlipName].uGuid) then
      Object.Remove(tEvents[sBlipName].uGuid)
    end
    Event.Delete(tEvents[sBlipName].uHibernation)
    Event.Delete(tEvents[sBlipName].uDeath)
    Event.Delete(tEvents[sBlipName].uDriver)
    tEvents[sBlipName] = nil
  end
end

function AddAntiAir(uGuid, sLevel)
  if tAA[uGuid] then
    return
  end
  sLevel = string.lower(sLevel)
  tAA[uGuid] = {}
  tAA[uGuid].level = sLevel
  tAA[sLevel] = tAA[sLevel] + 1
  tRadarTextures = {
    basic = {texture = "radar_AA", offset = 0},
    medium = {texture = "radar_SAM", offset = 36},
    advanced = {texture = "radar_SAM", offset = 0},
    jammer = {
      texture = "radar_Jammer",
      offset = 72
    }
  }
end

function RemoveAntiAir(uGuid)
  if tAA[uGuid] then
    _RemoveBlipCallback(tostring(uGuid))
    local sLevel = tAA[uGuid].level
    tAA[sLevel] = tAA[sLevel] - 1
    tAA[uGuid] = nil
  end
end

function TestAALevel(sLevel)
  if sLevel == nil or sLevel == "none" then
    return
  end
  sLevel = string.lower(sLevel)
  if tAA[sLevel] and tAA[sLevel] > 0 then
    return sLevel
  elseif sLevel == "basic" then
    return TestAALevel("medium")
  else
    return
  end
end

function DenialMessage(sReason)
  Debug.Printf("<--> [PDA.Support.denied.denied]" .. tostring(sReason))
  local sMessage
  if sReason == "aa" then
    sMessage = "[PDA.Support.denied.basic]"
    PlayRandomVOCue({
      "Fiona.541168fio",
      "Fiona.541169fio",
      "Fiona.541170fio"
    })
  elseif sReason == "jammer" then
    sMessage = "[PDA.Support.denied.jammer]"
    PlayRandomVOCue({
      "Fiona.Support.Jammed01"
    })
  elseif sReason == "nodrop" then
    sMessage = "[PDA.Support.denied.dropzone]"
  elseif sReason == "nomunitions" then
    sMessage = "[PDA.Support.denied.nomunitions]"
  elseif sReason == "abortnodrop" then
    sMessage = "[PDA.Support.denied.dropzone]"
  elseif sReason == "abortdamage" then
    sMessage = "[PDA.Support.denied.damaged]"
  elseif sReason == "toomanysoldiersnil" then
    sMessage = "[PDA.Support.denied.toomanysoldiers]"
  elseif sReason == "noland" then
    sMessage = "[PDA.Support.denied.landingzone]"
  elseif sReason == "oilcon002_toofar" then
    sMessage = "[PDA.Support.denied.unclear]"
    MrxVoSequence.Start("Fiona-In-Mission-Contract-Oil02-95", nil, MrxVoSequence.knPriorityFreeplay)
  elseif sReason == "toomanysoldiersnil" then
    sMessage = "[PDA.Support.denied.toomanysoldiers]"
  elseif sReason == "toomanysoldiersAllied" then
    sMessage = "[PDA.Support.denied.toomanysoldiersAllied]"
  elseif sReason == "toomanysoldiersChina" then
    sMessage = "[PDA.Support.denied.toomanysoldiersChina]"
  elseif sReason == "toomanysoldiersGuerilla" then
    sMessage = "[PDA.Support.denied.toomanysoldiersGuerilla]"
  elseif sReason == "toomanysoldiersOC" then
    sMessage = "[PDA.Support.denied.toomanysoldiersOC]"
  elseif sReason == "toomanysoldiersPirate" then
    sMessage = "[PDA.Support.denied.toomanysoldiersPirate]"
  elseif sReason == "fuel" then
    sMessage = "[PDA.Support.denied.fuel]"
  end
  if sMessage then
    Hud.MessageBox:AddMessage({
      sMessage = "[red][PDA.Support.denied.denied] " .. sMessage,
      nDuration = 4
    })
  end
end

function SynchNetImportModule(sModule)
  dynamic_import(sModule)
end

function SynchNetAction(oModule, uModule, fX, fY, fZ, uDesignatorGuid, uTarget, uOwnerGuid, uCargo, uFinalDestination, uDeliveryVehicle, uSetBomb, bEventPost)
  if not tRemoteNetObjects[uModule] then
    tRemoteNetObjects[uModule] = oModule:Create(uOwnerGuid)
  end
  if uCargo then
    tRemoteNetObjects[uModule]:SetCargoGuid(uCargo)
  end
  if uFinalDestination then
    tRemoteNetObjects[uModule]:SetFinalDestination(uFinalDestination)
  end
  if uDeliveryVehicle then
    tRemoteNetObjects[uModule].uDeliveryVehicle = uDeliveryVehicle
  end
  if uSetBomb then
    tRemoteNetObjects[uModule].uBomb = uSetBomb
  end
  if tRemoteNetObjects[uModule].oDesignator then
    tRemoteNetObjects[uModule].oDesignator:SetDesignationParameters(fX, fY, fZ, uDesignatorGuid, uTarget)
    tRemoteNetObjects[uModule].oDesignator.bDesignationComplete = true
  end
  if bEventPost then
    Event.Post("Airstrike", {
      sStage = "DesignationComplete",
      sType = "None"
    })
  end
  tRemoteNetObjects[uModule]:DesignationCallback()
end

function SynchNetAddItem(oModule, uModule, aName, aIcon, aLitIcon)
  if not tLocalNetObjects[uModule] then
    tLocalNetObjects[uModule] = oModule:Create(Player.GetLocalPlayer())
  end
  local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", Player.GetLocalPlayer())
  if oSupportMenu then
    oSupportMenu:Open()
    oSupportMenu:AddItem({
      sName = aName,
      sIcon = aIcon,
      sLitIcon = aLitIcon,
      oSupport = tLocalNetObjects[uModule]
    })
    Event.Create(Event.TimerRelative, {1}, oSupportMenu.Close, {oSupportMenu})
  end
end

function SynchNetRemoveItem(aName)
  local oSupportMenu = MrxGui.GetWidgetByNameAndOwner("Support Menu", Player.GetLocalPlayer())
  if oSupportMenu then
    oSupportMenu:RemoveItem(aName)
  end
end

function SetDeliveryVehicle(self, sVehicleTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sVehicleTemplateName) then
    return
  end
  self.sDeliveryVehicle = sVehicleTemplateName
  self.uDeliveryVehicle = Pg.GetGuidByName(sVehicleTemplateName)
end

function SetBomb(self, sBombTemplateName)
  if "table" ~= type(self) then
    return
  end
  if "string" ~= type(sBombTemplateName) then
    return
  end
  self.sBomb = sBombTemplateName
  self.uBomb = Pg.GetGuidByName(sBombTemplateName)
end

function RefundCosts(self)
  if self.nFuelConsumed then
    MrxPmc.AddFuelQty(self.nFuelConsumed)
    self.nFuelConsumed = nil
  end
  if self.sStockpileConsumed then
    MrxPmc.AddSupportQty(self.sStockpileConsumed, 1)
    self.sStockpileConsumed = nil
  end
  if self.sFreebieConsumed then
    MrxPmc.AddFreebieQty(self.sFreebieConsumed, 1)
    self.sFreebieConsumed = nil
  end
end

function SetupDamageEvent(self, uHeli, bCompleted)
  Debug.Printf("SetupDamageEvent for " .. tostring(uHeli))
  Ai.SetPriorityTarget(uHeli)
  if self:GetRecruit() == "Copter" then
    Event.Create(Event.ScriptEvent, {
      "RecruitAvailable",
      function(tData)
        if tData and tData[1] == "Copter" then
          return true
        end
      end
    }, FadeOut, {uHeli})
  end
  return Event.Create(Event.ObjectHealth, {
    uHeli,
    "<",
    Object.GetHealth(uHeli) * 0.6
  }, Abort, {self, uHeli})
end

function Abort(self, uHeli, sReason)
  Debug.Printf("ABORTING SUPPORT")
  if self.bSupportAborted then
    return
  else
    self.bSupportAborted = true
  end
  Object.DetachCargoFromWinch(self.uHeli)
  if not sReason and not self.bSupportComplete then
    Hud.MessageBox:AddMessage({
      sMessage = "[red][PDA.Support.denied.damaged]",
      nDuration = 4
    })
    Debug.Printf("--> COPTER DAMAGED")
  end
  if sReason and sReason == "NoMunitions" then
    Hud.MessageBox:AddMessage({
      sMessage = "[red][PDA.Support.denied.notarget]",
      nDuration = 4
    })
    Debug.Printf("--> NO MUNITIONS FOUND TO PICK UP")
  else
    tVO = {
      PMC = {
        "Ewan.Support.Denial10",
        "Ewan.Support.Denial11",
        "Ewan.Support.Denial12",
        "Ewan.Support.Denial10",
        "Ewan.Support.Denial11",
        "Ewan.Support.Denial12",
        {
          "Fiona.Support.Denial01",
          0,
          "Ewan.Support.Denial03",
          1,
          {
            jennifer = "Jen.Support.Denial01",
            chris = "Chris.Support.Denial01",
            mattias = "Mattias.Support.Denial01"
          }
        }
      },
      Allied = {
        "AlliedSoldier01.Support.Denied01"
      },
      China = {
        "ChinaSoldier01.Support.Denied01"
      },
      Guerilla = {
        "GurSoldier01.Support.Denied01"
      },
      OC = {
        "OCSoldier01.Support.Denied01"
      },
      Pirate = {
        "Fiona.PirateCoverage.Reinforcements01"
      }
    }
    local sFaction = MrxUtil.GetFaction(uHeli)
    if sFaction and tVO[sFaction] then
      PlayRandomVOCue(tVO[sFaction])
    end
    if sFaction == "PMC" then
      local nHealth = Object.GetHealth(uHeli)
      local nMaxHealth = Object.GetMaxHealth(uHeli)
      local nPenalty = nMaxHealth - nHealth * 200
      MrxPmc.AddCashQty(nPenalty, true, "[Generic.CopterRepair]")
    end
  end
  GoHome(self, uHeli)
end

function SetupPilotKilledEvent(self, uHeli, bCompleted)
  local uPilot = Vehicle.GetDriver(uHeli)
  Event.Create(Event.ObjectDeath, {uPilot}, Abandon, {self, uHeli})
end

function Abandon(self, uHeli)
  Debug.Printf("MrxSupport.Abandon (driver killed or hijacked?)")
  Object.DetachCargoFromWinch(uHeli)
  if sFaction == "PMC" then
    local nPenalty = 0
  end
  self.bSupportComplete = true
  MrxSupportManager.MakeRecruitAvailable("Copter")
end

function GoHome(self, uGuid, uWinchedGuid)
  Debug.Printf("MrxSupport.GoHome")
  self.bSupportComplete = true
  local uDriver = Vehicle.GetDriver(uGuid)
  if uDriver and Object.IsAlive(uDriver) then
    local nTargetX, nTargetY, nTargetZ
    tLocs = {
      PMC = "01_pmc_hq_lz_playerone",
      Allied = "07_all_hq_lz_playerone",
      China = "12_chi_hq_lz_playerone",
      Guerilla = "05_gur_hq_lz_playerone",
      OC = "02_oil_hq_lz_playerone",
      Pirate = "08_pir_hq_lz_playerone"
    }
    local sFaction = MrxUtil.GetFaction(uGuid)
    local uLoc
    if sFaction and tLocs[sFaction] then
      uLoc = Pg.GetGuidByName(tLocs[sFaction])
      if uLoc then
        if Object.GetDistanceFrom(uGuid, uLoc) > 20 then
          nTargetX, nTargetY, nTargetZ = Object.GetPosition(uLoc)
        else
          nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-600, 200, -1)
        end
      end
    end
    if not uLoc then
      Debug.Printf("Landing Zone not found, falling back to random location (" .. tostring(tLocs[sFaction]) .. ")")
      nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-300, 25)
    end
    if nTargetX then
      uGoal = Ai.Goal({
        AIGuid = uDriver,
        Goal = "MoveTo",
        Location = {
          nTargetX,
          nTargetY + 30,
          nTargetZ
        },
        Priority = "hiPri",
        Callback = Land,
        CallbackData = {
          self,
          uGuid,
          nTargetX,
          nTargetY,
          nTargetZ,
          uWinchedGuid
        },
        Force = true
      })
    else
      Debug.Printf("Unable to find position for " .. tostring(uLoc))
    end
    Event.Create(Event.ObjectHibernation, {uGuid, "hibernated"}, Object.Remove, {uGuid})
    Event.Create(Event.ObjectDelete, {uGuid}, MrxSupportManager.MakeRecruitAvailable, {"Copter"})
    if uWinchedGuid then
      Event.Create(Event.ObjectHibernation, {uWinchedGuid, "hibernated"}, Object.Remove, {uWinchedGuid})
    end
  end
end

function Land(self, uGuid, nTargetX, nTargetY, nTargetZ, uWinchedGuid)
  Debug.Printf("MrxSupport.Land")
  local uDriver = Vehicle.GetDriver(uGuid)
  if uWinchedGuid and Object.IsWinched(uWinchedGuid) then
    Object.FadeOut(uWinchedGuid, 1, true)
  end
  if uDriver and Object.IsAlive(uDriver) then
    local goal = Ai.Goal({
      AIGuid = uDriver,
      Goal = "HeliLand",
      Location = {
        nTargetX,
        nTargetY,
        nTargetZ
      },
      Priority = "hiPri",
      force = true,
      Callback = FadeOut,
      CallbackData = {uGuid}
    })
  else
    FadeOut(uGuid)
  end
end

function FadeOut(uGuid, nState)
  if not Object.IsValid(uGuid) then
    return
  end
  local uDriver = Vehicle.GetDriver(uGuid)
  if not uDriver then
    return
  end
  if uDriver and Object.HasLabel(uDriver, "Hero") then
    return
  end
  Debug.Printf("Removing Copter (received state " .. tostring(nState) .. ")")
  Object.DetachCargoFromWinch(uGuid)
  Ai.Deploy({
    Vehicle = uGuid,
    Role = "Passenger",
    Force = true
  })
  Object.FadeOut(uGuid, 2, true)
  if Vehicle.GetDriver(uGuid) then
    Object.FadeOut(Vehicle.GetDriver(uGuid), 2, true)
  end
end

function PlayAirstrikeVO(uJet, sMisha)
  tVOOnTheWay = {
    PMC = sMisha,
    Allied = {
      "AlliedSoldier01.Support.Incoming01",
      "AlliedSoldier01.Support.Incoming02",
      "AlliedSoldier01.Support.Incoming03"
    },
    China = {
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    },
    VZ = {
      "VZSoldier01.Support.Air01"
    },
    Guerilla = {
      "GurSoldier01.Support.Incoming01",
      "GurSoldier01.Support.Incoming02",
      "GurSoldier01.Support.Incoming03"
    },
    OC = {
      "OCSoldier01.Support.Incoming01",
      "OCSoldier01.Support.Incoming02",
      "OCSoldier01.Support.Incoming03"
    }
  }
  local sFaction = MrxUtil.GetFaction(uJet)
  Debug.Printf("Playing VO Cue for " .. tostring(sFaction))
  if sFaction and tVOOnTheWay[sFaction] then
    PlayRandomVOCue(tVOOnTheWay[sFaction], false)
  else
    Debug.Printf("No cue found for " .. tostring(tVOOnTheWay[sFaction]))
  end
end

function GetSpawnHeight()
  if Player.GetSecondaryCharacter() then
    return 250
  else
    return 50
  end
end
