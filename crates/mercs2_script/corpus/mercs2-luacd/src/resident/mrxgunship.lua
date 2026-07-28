inherit("MrxSupport")
import("MrxSupportDesignatorSmoke")
import("MrxUtil")

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorSmoke:Create()
  oDesignator:SetSmokeColor("red")
  oDesignator:SetAATestLevel("basic")
  oDesignator:SetValidationFunction(_NoValidation)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetModuleName("MrxGunship")
  return oNewSupport
end

function DesignationCallback(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(-300, 60, -1, self.uOwner)
  local nTargetX, nTargetY, nTargetZ = Pg.FindPointFromCamera(-200, 60, -1, self.uOwner)
  local nHeroX, nHeroY, nHeroZ = Object.GetPosition(self.uOwner)
  local nCameraX, nCameraY, nCameraZ = self:GetDesignator():GetTarget()
  local nHeightVectorX, nHeightVectorY, nHeightVectorZ = nCameraX - nHeroX, nCameraY - nHeroY, nCameraZ - nHeroZ
  nHeightVectorX, nHeightVectorY, nHeightVectorZ = Math.Normalize(nHeightVectorX, nHeightVectorY, nHeightVectorZ)
  local nWidthVectorX, nWidthVectorY, nWidthVectorZ = -(nCameraZ - nHeroZ), 0, nCameraX - nHeroX
  nWidthVectorX, nWidthVectorY, nWidthVectorZ = Math.Normalize(nWidthVectorX, nWidthVectorY, nWidthVectorZ)
  MrxVoSequence.Start("Fiona-In-Mission-Contract-All03-20", nil, MrxVoSequence.knPriorityFreeplay)
  self.uJet = Airstrike.Flyby("Support Vehicle (AC130)", nSpawnX, nSpawnZ, nTargetX - nWidthVectorX * 30, nTargetZ - nWidthVectorZ * 30, nTargetY, 45, Salvo, {self})
end

function Salvo(self, uLastTarget)
  local nSpawnX, nSpawnY, nSpawnZ = self:GetDesignator():GetTarget()
  if not (nSpawnX and Object.IsAwake(self.uJet)) or MrxUtil.GetDistanceBetween(self.uJet, Player.GetLocalCharacter()) > 300 then
    return
  end
  local tTargets = Pg.GetAwakeObjects(nSpawnX, nSpawnY, nSpawnZ, 100)
  for i, target in pairs(tTargets) do
    if target ~= uLastTarget and Object.IsAlive(target) and (Object.HasLabel(target, "VZ") or Object.HasLabel(target, "China") or Object.HasLabel(target, "Guerilla")) then
      uTarget = target
      break
    end
  end
  for i = 1, 4 do
    Event.Create(Event.TimerRelative, {
      0.25 * i
    }, LaunchMissile, {self, uTarget})
  end
  Event.Create(Event.TimerRelative, {3}, Salvo, {self, uTarget})
end

function LaunchMissile(self, uTarget)
  if not uTarget then
    return
  end
  local nSpawnX, nSpawnY, nSpawnZ = Object.GetPosition(self.uJet)
  local nTargetX, nTargetY, nTargetZ = Object.GetPosition(uTarget)
  if not nTargetX or not nSpawnX then
    return
  end
  nTargetX = nTargetX + math.randi(25) - math.randi(25)
  nTargetZ = nTargetZ + math.randi(25) - math.randi(25)
  local nVectorX, nVectorY, nVectorZ = nTargetX - nSpawnX, nTargetY - nSpawnY, nTargetZ - nSpawnZ
  nVectorX, nVectorY, nVectorZ = Math.Normalize(nVectorX, nVectorY, nVectorZ)
  Sound.CueSound(self.uJet, "wpn_tankgun_fire_npc")
  Pg.Spawn("global_particle_muzzleflash_tank", nSpawnX, nSpawnY, nSpawnZ)
  local nSpeedScale = 100
  Airstrike.SpawnOrdnance("Gunship Shell", nSpawnX, nSpawnY, nSpawnZ, nVectorX * nSpeedScale, nVectorY * nSpeedScale, nVectorZ * nSpeedScale, "impact", 1, uPlayer)
end

function _ValidateDropZone(fCallback, nX, nY, nZ, oSupport)
  local bDropZoneAdded = Ai.TestDropZone({
    Callback = fCallback,
    Location = {
      nX,
      nY,
      nZ
    },
    InnerRadius = 1,
    InnerHeightTolerance = 1,
    OuterRadius = 2,
    OuterHeightTolerance = 2,
    HeightMax = 5,
    SearchRadius = 12,
    Water = false
  })
  if not bDropZoneAdded then
    fCallback(false, "noland")
  end
end
