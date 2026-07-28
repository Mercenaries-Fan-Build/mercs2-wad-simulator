import("MrxSupport")
inherit("MrxSupportDesignator")
import("MrxGuiManager")
import("MrxSupportManager")
import("MrxPmc")
import("MrxGuiSatellite")
import("MrxSound")
nStartZoom = 170
nMinZoom = 170
nMaxZoom = 170
nRadius = 100
tSectors = false
nCost = 5000

function SetZoomLimits(self, nMinZoom, nMaxZoom, nStartZoom)
  self.nMinZoom = nMinZoom or self.nMinZoom
  self.nMaxZoom = nMaxZoom or self.nMaxZoom
  self.nStartZoom = nStartZoom or self.nStartZoom
end

function SetRadius(self, nRadius)
  self.nRadius = nRadius or self.nRadius
end

function SetMinigameSectors(self, tSectorData)
  self.tSectors = tSectorData
end

function SetCost(self, nCost)
  self.nCost = nCost
end

function ShouldSuppressIconAnimationOnDirectUse(self)
  return true
end

function Create(self, oNewDesignator)
  oNewDesignator = oNewDesignator or {}
  oNewDesignator.uOwner = self.uOwner
  oNewDesignator.bDesignationComplete = self.bDesignationComplete
  oNewDesignator.sDesignationType = "Satellite Designator"
  oNewDesignator.fValidationFunction = self.fValidationFunction
  oNewDesignator.tCallbackList = {}
  oNewDesignator.sAATestLevel = bil
  oNewDesignator.nX = self.nX
  oNewDesignator.nY = self.nY
  oNewDesignator.nZ = self.nZ
  oNewDesignator.uGuid = self.uGuid
  oNewDesignator.nStartZoom = self.nStartZoom
  oNewDesignator.nMinZoom = self.nMinZoom
  oNewDesignator.nMaxZoom = self.nMaxZoom
  oNewDesignator.nRadius = self.nRadius
  oNewDesignator.tSectors = self.tSectors
  oNewDesignator.nCost = self.nCost
  setmetatable(oNewDesignator, self)
  self.__index = self
  return oNewDesignator
end

function Commence(self, bFireImmediately)
  if "userdata" ~= type(self.uOwner) then
    return
  end
  return Airstrike.EquipDesignator(self.uOwner, self.sDesignationType, BeginSatelliteDesignation, {self}, bFireImmediately)
end

function GetType(self)
  return "satellite"
end

function BeginSatelliteDesignation(self)
  if "userdata" ~= type(self.uOwner) then
    return
  end
  MrxSound.EnterSatelliteView()
  local nX, nY, nZ = Object.GetPosition(Player.GetCharacter(self.uOwner))
  Player.SetPDAMapMode(self.uOwner, true, nX, nY + self.nStartZoom, nZ, self.nRadius, self.nStartZoom - self.nMinZoom, self.nMaxZoom - self.nStartZoom, MrxGuiSatellite.UseMinigame())
  Player.SetPDAMapModeCallback(self.uOwner, true, SatelliteTargettingEnd, {self})
  MrxGuiManager.ToggleSatellite(self.uOwner, true, "pmc")
  Player.SetPDAMapModeCancelCallback(self.uOwner, SatelliteTargettingCancel, {self})
  if MrxGuiSatellite.UseMinigame() then
    MrxGuiManager.SetSatelliteSuccessCallback(self.uOwner, SatelliteTargettingEnd, {self})
    if self.tSectors then
      MrxGuiManager.SetSatelliteMinigameData(self.uOwner, self.tSectors)
    end
    MrxGuiManager.SetSatelliteCost(self.uOwner, self.nCost)
  end
end

function SatelliteTargettingEnd(oDesignator, uGuid, x, y, z)
  MrxGuiManager.ToggleSatellite(oDesignator.uOwner, false)
  Player.SetPDAMapMode(oDesignator.uOwner, false)
  Player.SetPDAMapModeCallback(oDesignator.uOwner, true, DoNothing, {})
  oDesignator:SetTargetLocation(x, y, z)
  MrxSound.ExitSatelliteView()
  Event.Create(Event.TimerRelative, {0.2}, PostEndStep, {oDesignator})
end

function SatelliteTargettingCancel(oDesignator)
  MrxGuiManager.ToggleSatellite(oDesignator.uOwner, false)
  Player.SetPDAMapMode(oDesignator.uOwner, false)
  Player.SetPDAMapModeCallback(oDesignator.uOwner, true, DoNothing, {})
  MrxSound.ExitSatelliteView()
end

function PostEndStep(oDesignator)
  if oDesignator.sAATestLevel and MrxSupport.TestAALevel(oDesignator.sAATestLevel) then
    MrxSupport.DenialMessage("aa")
    return
  end
  if oDesignator:GetParentSupport():GetFuelCost() > MrxPmc.GetFuelQty() then
    MrxSupport.DenialMessage("fuel")
    return
  end
  if MrxSupportManager.IsRecruitAvailable(oDesignator:GetParentSupport():GetRecruit()) then
    oDesignator:CompleteDesignation()
    MrxSupportManager.StartRecruitCooldown(oDesignator:GetParentSupport():GetRecruit())
  end
end

function _DelayDesignationComplete(oDesignator)
  if MrxSupportManager.IsRecruitAvailable(oDesignator:GetParentSupport():GetRecruit()) then
    oDesignator:CompleteDesignation()
    MrxSupportManager.StartRecruitCooldown(oDesignator:GetParentSupport():GetRecruit())
  end
end

function DoNothing()
end
