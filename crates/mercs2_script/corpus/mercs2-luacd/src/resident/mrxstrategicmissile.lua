inherit("MrxSupport")
import("MrxSupportDesignatorBeacon")
local sProjectileName = "Strategic Missile Projectile"
local sProjectileLaunchName = "Strategic Missile Projectile Launch"
local sShrapnelName = "Strategic Missile Shrapnel"
local sExplosionName = "Explosion (Strategic Missile)"

function Create(self, uPlayerGuid)
  local oNewSupport = {}
  setmetatable(oNewSupport, self)
  self.__index = self
  local oDesignator = MrxSupportDesignatorBeacon:Create()
  oNewSupport:SetDesignator(oDesignator)
  oNewSupport:SetOwner(uPlayerGuid)
  oNewSupport:SetRecruit("Fiona")
  oNewSupport:SetModuleName("MrxStrategicMissile")
  return oNewSupport
end

function DesignationCallback(self)
  MrxVoSequence.Start({
    MrxUtil.GetRandomTableElement({
      "ChinaSoldier01.Support.Artillery01",
      "ChinaSoldier01.Support.Incoming01",
      "ChinaSoldier01.Support.Incoming02",
      "ChinaSoldier01.Support.Incoming03"
    }),
    {
      MissileLaunch,
      {self}
    }
  }, nil, MrxVoSequence.knPriorityFreeplay, false)
end

function MissileLaunch(self)
  local nSpawnX, nSpawnY, nSpawnZ = Pg.FindPointFromCamera(150, 10, -1, self.uOwner)
  local uOrdnanceGuid = Airstrike.SpawnOrdnance(sProjectileLaunchName, nSpawnX, nSpawnY, nSpawnZ, 0, 100, 0, "distance", 500, self.uOwner, ActivateDelay, {self})
end

function ActivateDelay(self, uProjectileGuid)
  Event.Create(Event.TimerRelative, {2}, self.DropBomb, {self})
end

function DropBomb(self)
  local nTargetX, nTargetY, nTargetZ, uDesignatorGuid = self:GetDesignator():GetTarget()
  nTargetY = nTargetY + 100
  self.uSpawnedBomb = Airstrike.SpawnOrdnance(sProjectileName, nTargetX, nTargetY, nTargetZ, 0, -100, 0, "distance", 80, self.uOwner, BombExplodes, {self})
end

function BombExplodes(self)
  local nBombX, nBombY, nBombZ = Object.GetPosition(self.uSpawnedBomb)
  local nTargetX, nTargetY, nTargetZ, uGuid, uTarget = self:GetDesignator():GetTarget()
  Debug.Printf("nTargetX: " .. tostring(nTargetX))
  Debug.Printf("nTargetY: " .. tostring(nTargetY))
  Debug.Printf("nTargetZ: " .. tostring(nTargetZ))
  Debug.Printf("uGuid: " .. tostring(uGuid))
  Debug.Printf("uTarget: " .. tostring(uTarget))
  Pg.Spawn(sExplosionName, nBombX, nBombY, nBombZ)
  Object.Kill(self.uSpawnedBomb)
  if uGuid and Player.IsLocal(self.uOwner) then
    Object.Remove(uGuid)
  end
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 5, 20, 2)
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 30, 20, 3)
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 45, 20, 3)
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 60, 20, 3)
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 75, 20, 3)
  Airstrike.ConeSpawn(sShrapnelName, nBombX, nBombY, nBombZ, nTargetX - nBombX, nTargetY - nBombY, nTargetZ - nBombZ, 90, 20, 4)
end
