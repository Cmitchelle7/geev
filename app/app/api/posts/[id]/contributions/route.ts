import { NextRequest } from 'next/server';
import { prisma } from '@/lib/prisma';
import { apiSuccess, apiError } from '@/lib/api-response';
import { getCurrentUser } from '@/lib/auth';
import { readJsonBody } from '@/lib/parse-json-body';
import { XP_REWARDS, awardXp } from '@/lib/xp';
import { checkAndAwardBadges } from '@/lib/badges';

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> },
) {
  try {
    const currentUser = await getCurrentUser(request);
    if (!currentUser) return apiError('Unauthorized', 401);

    const { id: postId } = await params;
    const body = await readJsonBody<Record<string, unknown>>(request);
    if (!body.ok) return body.response;

    const { amount, message, isAnonymous } = body.data as any;
    const parsedAmount = Number(amount);

    if (isNaN(parsedAmount) || parsedAmount <= 0) {
      return apiError('Amount must be greater than 0', 400);
    }

    const post = await prisma.post.findUnique({
      where: { id: postId },
      select: { id: true, type: true, targetAmount: true, status: true, userId: true },
    });

    if (!post) {
      return apiError('Post not found', 404);
    }

    if (post.type !== 'request') {
      return apiError('Contributions can only be made to help requests', 400);
    }

    if (!['open', 'in_progress', 'active'].includes(post.status)) {
      return apiError('Help request is not accepting contributions', 400);
    }

    const contribution = await prisma.$transaction(async (tx) => {
      const created = await tx.helpContribution.create({
        data: {
          postId,
          userId: currentUser.id,
          amount: parsedAmount,
          message: message || null,
          isAnonymous: !!isAnonymous,
          contributedAt: new Date(),
        },
        include: {
          user: {
            select: { id: true, name: true, avatarUrl: true, rank: true }
          }
        }
      });

      await awardXp(
        currentUser.id,
        XP_REWARDS.contributeToHelpRequest,
        'help_request_contributed',
        { metadata: { postId, contributionId: created.id } },
        tx
      );

      await awardXp(
        post.userId,
        XP_REWARDS.receiveContribution,
        'help_request_received_contribution',
        { metadata: { postId, contributionId: created.id } },
        tx
      );

      if (post.userId !== currentUser.id && tx.notification?.create) {
        try {
          await tx.notification.create({
            data: {
              userId: post.userId,
              type: 'help_contribution',
              message: `${isAnonymous ? 'Someone' : currentUser.name} contributed to yourhelp request`,
              link: `/post/${postId}`,
            }
          });
        } catch(e) { /* ignore if table missing */ }
      }

      return created;
    });

    checkAndAwardBadges(currentUser.id).catch(console.error);

    const aggregate = await prisma.helpContribution.aggregate({
      where: { postId },
      _sum: { amount: true }
    });
    const totalRaised = aggregate._sum.amount || 0;

    // FIX: Mask anonymous contribution in POST response
    let maskedContribution = contribution;
    if (contribution.isAnonymous) {
      const { user, userId, ...rest } = contribution;
      maskedContribution = {
        ...rest,
        user: { id: 'anonymous', name: 'Anonymous', avatarUrl: null, rank: null }
      } as any;
    }

    return apiSuccess({
      contribution: maskedContribution,
      progress: {
        totalRaised,
        targetAmount: post.targetAmount,
        percentage: post.targetAmount ? Math.min(100, Math.round((totalRaised / post.targetAmount) * 100)) : 0
      }
    }, 'Contribution successful', 201);
  } catch (error) {
    console.error('Contribution error:', error);
    return apiError('Failed to process contribution', 500);
  }
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> },
) {
  try {
    const { id: postId } = await params;

    const post = await prisma.post.findUnique({
      where: { id: postId },
      select: { id: true }
    });

    if (!post) {
      return apiError('Post not found', 404);
    }

    const contributions = await prisma.helpContribution.findMany({
      where: { postId },
      include: {
        user: {
          select: { id: true, name: true, avatarUrl: true, rank: true }
        }
      },
      orderBy: { contributedAt: 'desc' }
    });

    const aggregate = await prisma.helpContribution.aggregate({
      where: { postId },
      _sum: { amount: true }
    });
    const totalRaised = aggregate._sum.amount || 0;

    // FIX: Strip userId and mask user for anonymous contributions
    return apiSuccess({
      contributions: contributions.map(c => {
        if (c.isAnonymous) {
          const { user, userId, ...rest } = c;
          return {
            ...rest,
            user: { id: 'anonymous', name: 'Anonymous', avatarUrl: null, rank: null }
          };
        }
        return c;
      }),
      totalRaised
    });
  } catch (error) {
    console.error('Fetch contributions error:', error);
    return apiError('Failed to fetch contributions', 500);
  }
}